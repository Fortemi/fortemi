#!/bin/bash
# Create required PostgreSQL extensions as superuser.
# This script runs automatically via /docker-entrypoint-initdb.d/ when
# the PostgreSQL container initializes a new database.
#
# Extensions:
#   - vector:  pgvector for embedding similarity search
#   - postgis: PostGIS for spatial/geographic queries
#   - pg_trgm: Trigram matching for emoji/symbol search (built-in)

set -e

psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<-EOSQL
    CREATE EXTENSION IF NOT EXISTS vector;
    CREATE EXTENSION IF NOT EXISTS postgis;
    CREATE EXTENSION IF NOT EXISTS pg_trgm;
EOSQL
