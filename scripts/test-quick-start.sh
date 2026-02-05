#!/usr/bin/env bash
#
# Quick start script for running tests locally
#
# Usage:
#   ./scripts/test-quick-start.sh [fast|integration|slow|coverage|all]
#
# Examples:
#   ./scripts/test-quick-start.sh fast        # Run fast tests only
#   ./scripts/test-quick-start.sh integration # Run integration tests
#   ./scripts/test-quick-start.sh coverage    # Generate coverage report
#   ./scripts/test-quick-start.sh all         # Run all tests

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
DB_CONTAINER="matric-test-db-$$"
DB_PORT="${TEST_DB_PORT:-15432}"
DATABASE_URL="postgres://matric:matric@localhost:${DB_PORT}/matric_test"

# Docker image for test database (pgvector + PostGIS)
# Default: builds local image from build/Dockerfile.testdb
# Override with TESTDB_IMAGE env var to use pre-built image
TESTDB_IMAGE="${TESTDB_IMAGE:-}"
LOCAL_TESTDB_IMAGE="matric-testdb:local"

# Print colored message
print_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check prerequisites
check_prerequisites() {
    # Check Docker is available
    if ! command -v docker &> /dev/null; then
        print_error "Docker is not installed or not in PATH"
        print_error "Please install Docker: https://docs.docker.com/get-docker/"
        exit 1
    fi

    # Check Docker daemon is running
    if ! docker info &> /dev/null; then
        print_error "Docker daemon is not running"
        print_error "Please start Docker and try again"
        exit 1
    fi
}

# Build local test database image if needed
ensure_testdb_image() {
    # If TESTDB_IMAGE is set, use it directly
    if [ -n "$TESTDB_IMAGE" ]; then
        print_info "Using specified image: $TESTDB_IMAGE"
        return 0
    fi

    # Check if local image already exists
    if docker image inspect "$LOCAL_TESTDB_IMAGE" &> /dev/null; then
        TESTDB_IMAGE="$LOCAL_TESTDB_IMAGE"
        print_info "Using existing local image: $TESTDB_IMAGE"
        return 0
    fi

    # Check if Dockerfile exists
    local dockerfile="build/Dockerfile.testdb"
    if [ ! -f "$dockerfile" ]; then
        print_warn "Local Dockerfile not found: $dockerfile"
        print_warn "Falling back to pgvector/pgvector:pg16 (no PostGIS support)"
        TESTDB_IMAGE="pgvector/pgvector:pg16"
        return 0
    fi

    # Build the local image
    print_info "Building local test database image (first time only)..."
    print_info "This includes pgvector + PostGIS for full migration support"
    if docker build -f "$dockerfile" -t "$LOCAL_TESTDB_IMAGE" . > /dev/null 2>&1; then
        TESTDB_IMAGE="$LOCAL_TESTDB_IMAGE"
        print_info "Built image: $TESTDB_IMAGE"
    else
        print_error "Failed to build local testdb image"
        print_error "Trying with verbose output..."
        docker build -f "$dockerfile" -t "$LOCAL_TESTDB_IMAGE" .
        exit 1
    fi
}

# Cleanup function
cleanup() {
    if [ -n "$DB_CONTAINER" ] && [ -n "$CLEANUP_ENABLED" ]; then
        print_info "Cleaning up test database..."
        docker stop "$DB_CONTAINER" 2>/dev/null || true
        docker rm "$DB_CONTAINER" 2>/dev/null || true
    fi
}

# Setup trap for cleanup
trap cleanup EXIT

# Find an available port starting from the given base
find_available_port() {
    local base_port="${1:-15432}"
    local port=$base_port
    local max_attempts=100

    for ((i=0; i<max_attempts; i++)); do
        if ! ss -tuln 2>/dev/null | grep -q ":$port " && \
           ! docker ps --format '{{.Ports}}' 2>/dev/null | grep -q ":$port->"; then
            echo "$port"
            return 0
        fi
        port=$((port + 1))
    done

    print_error "Could not find available port after $max_attempts attempts"
    exit 1
}

# Start PostgreSQL container
start_postgres() {
    check_prerequisites
    ensure_testdb_image

    # Find available port if default is in use
    if [ -z "$TEST_DB_PORT" ]; then
        DB_PORT=$(find_available_port 15432)
        DATABASE_URL="postgres://matric:matric@localhost:${DB_PORT}/matric_test"
    fi

    print_info "Starting PostgreSQL with pgvector + PostGIS..."
    print_info "Using image: $TESTDB_IMAGE"
    print_info "Using port: $DB_PORT"

    # Stop any existing container with same name
    docker stop "$DB_CONTAINER" 2>/dev/null || true
    docker rm "$DB_CONTAINER" 2>/dev/null || true

    # Enable cleanup after container is created
    CLEANUP_ENABLED=1

    # Start new container using configured TESTDB_IMAGE
    if ! docker run -d --name "$DB_CONTAINER" \
        -p "${DB_PORT}:5432" \
        -e POSTGRES_USER=matric \
        -e POSTGRES_PASSWORD=matric \
        -e POSTGRES_DB=matric_test \
        "$TESTDB_IMAGE" >/dev/null 2>&1; then
        print_error "Failed to start container with image: $TESTDB_IMAGE"
        print_error "Try pulling the image first: docker pull $TESTDB_IMAGE"
        exit 1
    fi

    # Wait for PostgreSQL to be ready
    print_info "Waiting for PostgreSQL to be ready..."
    for i in {1..30}; do
        if docker exec "$DB_CONTAINER" pg_isready -U matric >/dev/null 2>&1; then
            print_info "PostgreSQL is ready!"
            break
        fi
        if [ $i -eq 30 ]; then
            print_error "PostgreSQL failed to start in time"
            print_error "Container logs:"
            docker logs "$DB_CONTAINER" 2>&1 | tail -20
            exit 1
        fi
        sleep 1
    done

    # Enable pgvector extension
    print_info "Enabling pgvector extension..."
    docker exec "$DB_CONTAINER" psql -U matric -d matric_test -c "CREATE EXTENSION IF NOT EXISTS vector;" >/dev/null 2>&1 || true

    # Run migrations
    print_info "Running database migrations..."
    local migration_count=0
    for migration in migrations/*.sql; do
        if [ -f "$migration" ]; then
            print_info "  → $(basename "$migration")"
            if ! docker exec -i "$DB_CONTAINER" psql -U matric -d matric_test -v ON_ERROR_STOP=1 < "$migration" >/dev/null 2>&1; then
                print_error "Migration failed: $(basename "$migration")"
                print_error "Run with VERBOSE=1 for details"
                if [ -n "$VERBOSE" ]; then
                    docker exec -i "$DB_CONTAINER" psql -U matric -d matric_test < "$migration"
                fi
                exit 1
            fi
            migration_count=$((migration_count + 1))
        fi
    done
    print_info "Applied $migration_count migrations"

    print_info "Database ready at $DATABASE_URL"
}

# Run fast tests (no database required)
run_fast_tests() {
    print_info "Running fast tests (unit tests)..."
    SKIP_INTEGRATION_TESTS=1 cargo test --lib --workspace

    print_info "Running doc tests..."
    SKIP_INTEGRATION_TESTS=1 cargo test --doc --workspace
}

# Run integration tests (database required)
run_integration_tests() {
    print_info "Running integration tests..."
    start_postgres
    export DATABASE_URL

    # Worker tests require serial execution due to shared database state
    # (tests share job queue, workers can claim jobs from other tests)
    # Run these FIRST on a clean database before other tests create data
    print_info "Running worker integration tests (serial)..."
    cargo test --package matric-jobs --test worker_integration_test -- --test-threads=1

    # Run all other integration tests
    print_info "Running other integration tests..."
    cargo test --workspace --tests --exclude matric-jobs
}

# Run slow/ignored tests
run_slow_tests() {
    print_info "Running slow tests..."
    start_postgres
    export DATABASE_URL
    cargo test --workspace -- --ignored
}

# Generate coverage report
run_coverage() {
    print_info "Generating coverage report..."

    # Check if cargo-llvm-cov is installed
    if ! command -v cargo-llvm-cov &> /dev/null; then
        print_warn "cargo-llvm-cov not found, installing..."
        cargo install cargo-llvm-cov
    fi

    start_postgres
    export DATABASE_URL

    # Create coverage directory
    mkdir -p target/coverage

    # Generate coverage
    print_info "Running tests with coverage instrumentation..."
    cargo llvm-cov --all-features --workspace --lcov --output-path target/coverage/lcov.info

    # Generate summary
    print_info "Generating coverage summary..."
    cargo llvm-cov report --summary-only | tee target/coverage/summary.txt

    print_info "Coverage report generated:"
    print_info "  → LCOV: target/coverage/lcov.info"
    print_info "  → Summary: target/coverage/summary.txt"

    # Optional: Generate HTML report
    if command -v genhtml &> /dev/null; then
        print_info "Generating HTML coverage report..."
        genhtml target/coverage/lcov.info -o target/coverage/html
        print_info "  → HTML: target/coverage/html/index.html"
    fi
}

# Run all tests
run_all_tests() {
    print_info "Running all tests..."
    run_fast_tests
    echo ""
    run_integration_tests
    echo ""
    print_info "All tests complete!"
}

# Print usage
usage() {
    echo "Usage: $0 [fast|integration|slow|coverage|all]"
    echo ""
    echo "Commands:"
    echo "  fast        - Run fast tests (no database required)"
    echo "  integration - Run integration tests (spins up PostgreSQL container)"
    echo "  slow        - Run slow/ignored tests (requires database)"
    echo "  coverage    - Generate coverage report (requires database)"
    echo "  all         - Run fast + integration tests"
    echo ""
    echo "Environment variables:"
    echo "  TEST_DB_PORT  - PostgreSQL port (default: auto-selects available port starting from 15432)"
    echo "  TESTDB_IMAGE  - Docker image for test DB (default: builds local image from build/Dockerfile.testdb)"
    echo "  VERBOSE       - Set to 1 for verbose migration output on failure"
    echo ""
    echo "Examples:"
    echo "  $0 fast                           # Quick unit tests, no Docker needed"
    echo "  $0 integration                    # Full integration tests with temp DB"
    echo "  TEST_DB_PORT=5433 $0 integration  # Use specific port"
    echo "  TESTDB_IMAGE=pgvector/pgvector:pg16 $0 integration  # Use public image (no PostGIS)"
    echo ""
    exit 1
}

# Main
main() {
    local command="${1:-all}"

    print_info "Fortémi test runner"
    print_info "==================="
    echo ""

    case "$command" in
        fast)
            run_fast_tests
            ;;
        integration)
            run_integration_tests
            ;;
        slow)
            run_slow_tests
            ;;
        coverage)
            run_coverage
            ;;
        all)
            run_all_tests
            ;;
        *)
            usage
            ;;
    esac

    echo ""
    print_info "✅ Tests completed successfully!"
}

main "$@"
