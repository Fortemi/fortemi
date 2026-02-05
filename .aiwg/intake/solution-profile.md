# Solution Profile

**Document Type**: Greenfield Library/Crate Profile
**Generated**: 2026-01-02

## Profile Selection

**Profile**: MVP

**Selection Logic** (automated based on inputs):
- **Prototype**: Timeline <4 weeks, no external users, experimental/learning, high uncertainty
- **MVP**: Timeline 1-3 months, initial users (internal or limited beta), proving viability ✓
- **Production**: Timeline 3-6 months, established users, revenue-generating or critical operations
- **Enterprise**: Compliance requirements (HIPAA/SOC2/PCI-DSS), >10k users, mission-critical, contracts/SLAs

**Chosen**: MVP

**Rationale**:
- **Timeline**: 6-8 weeks for v0.1.0 (core functionality extraction from HotM)
- **Users**: 1 initial consumer (HotM), expanding to 3-5 experimental agents within 6 months
- **Purpose**: Prove viability of matric-memory as independent library, validate API boundaries
- **Stage**: Inception phase, migrating existing code but establishing new project
- **Risk tolerance**: Can iterate and refactor post-v0.1.0, but HotM integration must succeed
- **Not Prototype**: Extracting proven code from HotM, not experimental
- **Not Production**: No established external users yet, API may evolve
- **Not Enterprise**: No compliance requirements, no SLAs, internal tooling

## Profile Characteristics

### Security

**Posture**: Baseline+ (Enhanced Baseline)

**Profile Defaults**:
- **Prototype/MVP**: Baseline (user auth, environment secrets, HTTPS, basic logging)
- **Production**: Strong (threat model, SAST/DAST, secrets manager, audit logs, incident response)
- **Enterprise**: Enterprise (full SDL, penetration testing, compliance controls, SOC2/ISO27001, IR playbooks)

**Chosen**: Baseline+ (Baseline with encryption enhancements)

**Rationale**:
- **Data sensitivity**: Internal-Confidential (notes may contain personal info, proprietary research)
- **Compliance**: None required (internal tooling)
- **User trust**: Developer library (consumers handle their own security)
- **Enhancement rationale**: Support dataset encryption for sensitive exports (AES-256)
- **No threat model needed**: Library crate, no network-facing attack surface (consumers handle that)
- **No audit logs needed**: Library logs to stdout, consumers can centralize if needed

**Controls Included**:

**Authentication**:
- Not applicable (library crate, no authentication layer)
- Consumer applications (like HotM) handle user authentication
- Database credentials: Environment variables (consumer responsibility)

**Authorization**:
- Not applicable initially (trust boundary is the consuming application)
- Future: Collection-level access control (API supports passing user/tenant context)

**Data Protection**:
- **Encryption at rest**: PostgreSQL database encryption (via LUKS, cloud provider encryption, or pgcrypto)
- **Encryption in transit**: PostgreSQL TLS connections (configurable, recommended for production consumers)
- **Dataset encryption**: AES-256 encryption for .matric package exports (optional, user-provided key)
- **Secrets**: Database URLs, API keys stored in environment variables (consumer responsibility)

**Secrets Management**:
- Environment variables for development (DATABASE_URL, OLLAMA_URL, OPENAI_API_KEY)
- Consumer applications should use proper secret management (Vault, AWS Secrets Manager, etc.)
- No secrets stored in matric-memory crate itself

**Dependency Security**:
- **Cargo audit**: Run in CI/CD pipeline, fail on high/critical vulnerabilities
- **SBOM**: Generate Software Bill of Materials (cargo audit --json)
- **Dependency review**: Minimize dependencies, prefer well-maintained crates with security track records

**Audit Logging**:
- Not included (library logs operations, consumers centralize logs if needed)
- Structured logging with tracing crate (consumer can instrument for audit if required)

**Gaps/Additions** (deviations from MVP baseline):
- **Addition**: Dataset encryption (AES-256 for .matric exports) - exceeds baseline due to data sensitivity
- **Addition**: Dependency scanning (cargo audit in CI) - best practice for library crates
- **Gap**: No audit logs - acceptable for library, consumers handle if needed
- **Gap**: No threat model - deferred to v0.2.0 if multi-tenant use cases emerge

### Reliability

**Targets**:

**Profile Defaults**:
- **Prototype**: 95% uptime, best-effort, no SLA
- **MVP**: 99% uptime, p95 latency <1s, business hours support ✓
- **Production**: 99.9% uptime, p95 latency <500ms, 24/7 monitoring, runbooks
- **Enterprise**: 99.99% uptime, p95 latency <200ms, 24/7 on-call, disaster recovery

**Chosen**: MVP with enhanced performance targets

**Targets**:
- **Availability**: 99% uptime (internal library, tolerates brief PostgreSQL restarts or Ollama outages)
- **Latency**: Hybrid search p95 <200ms for 10k corpus, <500ms for 100k corpus
- **Error Rate**: <1% failed searches (transient database errors acceptable, retries in consumer apps)
- **Throughput**: 10 searches/sec sustained, 100 documents/min ingest (background embeddings)

**Rationale**:
- **99% uptime**: Library dependency, brief outages acceptable (consumers can retry)
- **<200ms search**: MVP target for 10k documents (HotM use case), faster than baseline to prove performance viability
- **<1% errors**: Transient PostgreSQL or Ollama failures acceptable (job queue retries)
- **Solo developer**: No 24/7 support, business hours troubleshooting only

**Monitoring Strategy**:

**MVP Monitoring**:
- Structured logs + basic metrics (request count, latency, errors)

**Chosen**: Structured logging (tracing crate)
- **Logs**: JSON-formatted logs with context (document_id, query, duration, error)
- **Outputs**: stdout (consumer applications route to centralized logging)
- **Levels**: debug (development), info (runtime operations), warn (degraded), error (failures)
- **No APM**: Deferred to consumers (HotM can add Datadog/Sentry if needed)
- **No dashboards**: Use consumer application monitoring, or local log analysis tools

**Metrics** (optional, via tracing instrumentation):
- Search latency histogram (p50, p95, p99)
- Embedding generation duration
- Job queue depth and processing rate
- Database connection pool utilization

**Alerting**:
- Not applicable (library crate, consumers configure alerting)

### Testing & Quality

**Coverage Targets**:

**Profile Defaults**:
- **Prototype**: 0-30% (manual testing OK, fast iteration priority)
- **MVP**: 30-60% (critical paths covered, some integration tests) ✓
- **Production**: 60-80% (comprehensive unit + integration, some e2e)
- **Enterprise**: 80-95% (comprehensive coverage, full e2e, performance/load testing)

**Chosen**: 60% (upper end of MVP range)

**Rationale**:
- **Library crate**: Higher quality bar than applications (consumers depend on stability)
- **Migration risk**: Extracting from HotM, need tests to catch regressions
- **Public API**: All public functions must have unit tests (high value, low effort)
- **Integration tests**: Database and inference backends need real integration tests (not mocks)
- **60% achievable**: Solo developer with 6-8 week timeline can reach 60% for core paths
- **Defer edge cases**: 90%+ coverage deferred to v0.2.0 (diminishing returns for MVP)

**Test Types**:

**Unit Tests** (core coverage):
- All public API functions (matric-db, matric-search, matric-inference, matric-jobs, matric-datasets)
- Data model serialization/deserialization
- Search query builders and parsers
- Inference backend trait implementations
- Encryption/decryption for dataset packaging

**Integration Tests** (realistic scenarios):
- Database layer with real PostgreSQL + pgvector (Docker container)
- Search engine with indexed test corpus (FTS, semantic, hybrid)
- Inference backends with Ollama or mock server (HTTP)
- Job queue with background processing (async tests)
- End-to-end: ingest documents → generate embeddings → search (full pipeline)

**E2E Tests** (consumer perspective):
- HotM integration tests (matric-memory consumed as dependency)
- Real-world workflows: create collection, add documents, search, export dataset

**Performance Tests** (benchmarks, not continuous):
- Search latency benchmarks (10k, 100k document corpus)
- Embedding throughput benchmarks
- Database connection pool stress tests
- **Not load testing**: Deferred to Production profile (consumers handle scale testing)

**Security Tests**:
- Cargo audit (dependency vulnerabilities) in CI
- No SAST/DAST (deferred to Production profile, low attack surface for library)

**Quality Gates**:

**MVP Gates**:
- Linting, unit tests pass (CI required)

**Chosen** (enhanced MVP):
- **Linting**: rustfmt (formatting), clippy (idiomatic Rust)
- **Unit tests**: All tests pass (60% coverage target)
- **Integration tests**: Database, search, inference, job queue tests pass
- **Security scan**: cargo audit passes (no high/critical vulnerabilities)
- **Documentation**: Public API has rustdoc comments (cargo doc builds)
- **Code review**: Self-review (solo dev), automated linting catches common issues

**No Manual Gates** (for MVP):
- Performance benchmarks (run manually, not blocking)
- End-to-end tests with HotM (run before release, not every commit)

### Process Rigor

**SDLC Adoption**:

**Profile Defaults**:
- **Prototype**: Minimal (README, ad-hoc, trunk-based)
- **MVP**: Moderate (user stories, basic architecture docs, feature branches, PRs for review) ✓
- **Production**: Full (requirements docs, SAD, ADRs, test plans, runbooks, traceability)
- **Enterprise**: Enterprise (full artifact suite, compliance evidence, change control, audit trails)

**Chosen**: Moderate+ (Moderate with enhanced documentation)

**Rationale**:
- **Solo developer**: Lightweight process, no heavyweight governance
- **Library crate**: Higher documentation bar than applications (API docs critical for consumers)
- **Open-source potential**: May open-source in future, need quality docs and examples
- **ADRs important**: Architectural decisions need documentation (crate structure, database choice, inference abstraction)
- **No formal requirements**: Issues in Gitea serve as requirements backlog (issue #1-#20)

**Key Artifacts** (required for MVP+):

**Required**:
- **README.md**: Project overview, quick start, architecture diagram, contributing guide
- **API Documentation**: Inline rustdoc comments, cargo doc published to docs.rs (future)
- **Integration Guide**: Step-by-step for consumers (HotM as reference example)
- **ADRs** (Architecture Decision Records):
  - ADR-001: Multi-crate workspace structure
  - ADR-002: PostgreSQL + pgvector vs dedicated vector database
  - ADR-003: InferenceBackend trait abstraction
  - ADR-004: .matric dataset package format
- **CHANGELOG.md**: Track breaking changes, new features, bug fixes (semantic versioning)
- **Gitea Issues**: Requirements backlog (issue #1-#20, v0.1.0 milestone)

**Deferred** (not needed for MVP):
- Formal requirements documents (use Gitea issues instead)
- Software Architecture Document (SAD) - replace with ADRs + README architecture section
- Test strategy document - inline in README or CONTRIBUTING.md
- Runbook - library crate, consumers handle operational runbooks
- Governance docs - solo developer, no CCB/change control

**Git Workflow**:
- **Branching**: Feature branches from main (e.g., `feature/inference-abstraction`, `fix/search-crash`)
- **Protection**: Main branch protected (require CI passing, no force push)
- **Commits**: Conventional commits (feat/fix/docs/refactor)
- **Merge**: Squash merge to main (clean history)

**Documentation Standards**:
- **Rustdoc**: All public APIs have doc comments with examples
- **Examples**: `examples/` directory with runnable code (quick start, search, packaging)
- **README sections**: Overview, Features, Installation, Quick Start, Architecture, Contributing, License
- **ADR format**: Markdown, template with Context/Decision/Consequences sections

**Tailoring Notes**:
- **Lightweight for solo dev**: No formal PRs (self-review), no heavyweight process documents
- **Enhanced documentation**: API docs and integration guide exceed typical MVP (library crate needs it)
- **ADRs replace SAD**: Capture key decisions without heavyweight architecture document
- **Issues as requirements**: Gitea issues #1-#20 serve as structured backlog, no separate requirements docs

## Improvement Roadmap

**Phase 1 (Immediate - First Sprint, Weeks 1-2)**:

**Critical Setup**:
1. Create Cargo workspace structure (matric-core, matric-db, matric-search, matric-inference, matric-jobs, matric-datasets) - Issue #6
2. Set up CI/CD pipeline (Gitea Actions or GitHub Actions) - Issue #20
   - Lint (rustfmt, clippy)
   - Build (all crates)
   - Test (unit tests, integration tests with Docker)
   - Security (cargo audit)
3. Migrate database layer from HotM (matric-db crate) - Issue #3
4. Write README with architecture overview and quick start

**Documentation**:
1. Create ADR-001 (workspace structure decision)
2. Create ADR-002 (PostgreSQL + pgvector decision)
3. Rustdoc for public APIs in matric-db

**Testing**:
1. Unit tests for database layer (30%+ coverage)
2. Integration tests with PostgreSQL + pgvector (Docker)

**Outcome**: Foundational structure in place, database layer functional and tested

**Phase 2 (Short-term - Weeks 3-6, v0.1.0 Completion)**:

**Core Functionality**:
1. Implement search engine (matric-search) - Issue #4
2. Implement inference abstraction (InferenceBackend trait, Ollama backend) - Issues #7-9
3. Implement job queue system (matric-jobs) - Issue #5
4. Define public API surface (matric-core) - Issue #2
5. Migrate HotM to consume matric-memory as crate - HotM Issue #6

**Documentation**:
1. Integration guide for consumers (HotM as example) - Issue #19
2. API reference documentation (rustdoc) - Issue #18
3. ADR-003 (InferenceBackend abstraction)
4. Examples directory (basic usage, search, embeddings)

**Testing**:
1. Increase test coverage to 60% (search, inference, jobs)
2. Integration tests for full pipeline (ingest → embed → search)
3. HotM end-to-end tests (consumer validation)

**Outcome**: v0.1.0 release ready, HotM successfully consuming matric-memory, core functionality validated

**Phase 3 (Long-term - Post-v0.1.0, Weeks 7-12)**:

**Enhancements** (v0.2.0 scope):
1. Dataset packaging (.matric format, export/import/encryption) - Issues #12-16
2. OpenAI-compatible inference backend - Issue #10
3. Inference routing and configuration - Issue #11
4. Performance optimization (vector index tuning, caching)
5. Additional consumers (beyond HotM, agents)

**Documentation**:
1. Advanced integration guide (dataset packaging, multi-backend inference)
2. Performance tuning guide (index parameters, connection pooling)
3. Contributing guide (for potential open-source contributors)
4. ADR-004 (.matric package format)

**Testing**:
1. Performance benchmarks (10k, 100k, 1M document corpus)
2. Load testing (search throughput, embedding throughput)
3. Security review (dependency audit, consider SAST if multi-tenant)

**Quality Improvements**:
1. Increase test coverage to 70-80% (edge cases, error handling)
2. Fuzz testing for search query parser
3. Property-based testing for dataset serialization

**Outcome**: Mature library, multiple consumers, performance validated, ready for public release (crates.io)

## Overrides and Customizations

**Security Overrides**:
- **Override**: MVP profile but Baseline+ security (enhanced encryption)
- **Justification**: Document content may be sensitive (personal notes, proprietary research)
- **Addition**: Dataset encryption (AES-256 for .matric exports) - not typical for MVP
- **Addition**: Dependency scanning (cargo audit) - best practice for library crates
- **Revisit trigger**: If multi-tenant use cases emerge, upgrade to Strong security (threat model, access control)

**Reliability Overrides**:
- **Override**: MVP profile but <200ms search latency (tighter than baseline <1s)
- **Justification**: Performance is core value proposition (hybrid search must be fast)
- **Enhancement**: Performance benchmarks (not typical for MVP)
- **Revisit trigger**: If >100k documents, re-benchmark and optimize (vector index tuning)

**Testing Overrides**:
- **Override**: MVP profile but 60% coverage (upper end of 30-60% range)
- **Justification**: Library crate has higher quality expectations (consumers depend on stability)
- **Enhancement**: Integration tests with real PostgreSQL/Ollama (not mocked)
- **Revisit trigger**: If public release (crates.io), increase to 70-80% coverage

**Process Overrides**:
- **Override**: MVP profile but enhanced documentation (ADRs, API docs, integration guide)
- **Justification**: Library crate needs quality documentation for consumers
- **Tailoring**: Skip formal requirements docs (use Gitea issues as backlog)
- **Tailoring**: Skip governance docs (solo dev, no change control board)
- **Revisit trigger**: If open-source or multi-contributor, add CONTRIBUTING.md and governance

**Rationale for Overrides**:
- **Library crate context**: Higher quality bar than typical MVP application (API stability, docs, tests)
- **Migration context**: Extracting from HotM requires validation (integration tests, HotM end-to-end)
- **Solo developer**: Lightweight process, but automation and docs compensate for lack of team
- **Future-proofing**: Small enhancements now (ADRs, benchmarks) enable smooth v0.2.0+ evolution

## Key Decisions

**Decision #1: Profile Selection (MVP)**
- **Chosen**: MVP
- **Alternative Considered**: Production (given HotM dependency, could argue for higher rigor)
- **Rationale**:
  - **Timeline**: 6-8 weeks too tight for Production profile rigor
  - **Users**: Only 1 consumer initially (HotM), can iterate and refactor
  - **Risk tolerance**: Extracting proven code (not greenfield), API can evolve
  - **Not Prototype**: Proven code from HotM, need tests and docs (not throwaway)
  - **Not Production**: API not stabilized, no external users yet, no SLA
- **Revisit Trigger**: If 5+ consumers or public crates.io release, upgrade to Production profile

**Decision #2: Security Posture (Baseline+)**
- **Chosen**: Baseline+ (Baseline with dataset encryption)
- **Alternative Considered**: Baseline (standard MVP), Strong (if multi-tenant)
- **Rationale**:
  - **Data sensitivity**: Internal-Confidential (notes may contain PII, proprietary research)
  - **Encryption need**: Dataset exports (.matric) may be shared, need encryption option
  - **No compliance**: Internal tooling, no HIPAA/GDPR/SOC2 requirements
  - **No threat model**: Library crate, no network attack surface (consumers handle that)
  - **Cost/benefit**: AES-256 encryption low cost (well-supported in Rust), high user value
- **Revisit Trigger**: If multi-tenant use cases or compliance requirements emerge, upgrade to Strong security

**Decision #3: Test Coverage Target (60%)**
- **Chosen**: 60% (upper end of MVP 30-60% range)
- **Alternative Considered**: 30% (MVP baseline), 70-80% (Production)
- **Rationale**:
  - **Library quality**: Consumers depend on stability, higher bar than applications
  - **Migration risk**: Need tests to catch regressions from HotM extraction
  - **Achievable**: Solo dev with 6-8 weeks can reach 60% for core paths
  - **Diminishing returns**: Edge case testing (70%+) deferred to v0.2.0
  - **Integration tests**: Real PostgreSQL/Ollama tests critical (not mocked)
- **Revisit Trigger**: If public crates.io release, increase to 70-80% coverage

**Decision #4: Documentation Rigor (Enhanced MVP)**
- **Chosen**: Moderate+ (ADRs, API docs, integration guide)
- **Alternative Considered**: Minimal (README only), Full (SAD, test plans, runbooks)
- **Rationale**:
  - **Library needs docs**: API docs and integration guide are not optional (consumer enablement)
  - **ADRs capture decisions**: Lightweight alternative to heavyweight SAD
  - **Solo dev capacity**: Can't afford Production-level docs (no test plans, runbooks)
  - **Open-source potential**: Quality docs now enable future public release
- **Revisit Trigger**: If multi-contributor or enterprise consumers, add governance docs

**Decision #5: CI/CD Quality Gates (Enhanced MVP)**
- **Chosen**: Lint + tests + cargo audit (exceed baseline)
- **Alternative Considered**: Lint + tests only (MVP baseline), add SAST/DAST (Production)
- **Rationale**:
  - **Dependency security**: cargo audit low cost, high value (catch vulnerable deps early)
  - **Library distribution**: Consumers trust crate, need clean SBOM
  - **No SAST/DAST**: Low attack surface (library, not web app), deferred to Production
- **Revisit Trigger**: If public crates.io release, consider SAST (clippy covers most Rust issues)

## Next Steps

1. Review solution profile and validate that security/reliability/testing targets align with priorities from `option-matrix.md`
2. Confirm MVP profile with enhanced documentation and testing is achievable in 6-8 week timeline
3. Validate that Baseline+ security (dataset encryption, cargo audit) is sufficient for v0.1.0
4. Start Inception with MVP-appropriate templates and agents:
   - Architecture Designer (crate structure, inference abstraction)
   - Requirements Analyst (refine issues #1-#20 as needed)
   - Test Engineer (integration test strategy)
5. Revisit profile selection at phase transitions:
   - **Inception → Elaboration**: Confirm MVP still correct after architecture solidifies
   - **Elaboration → Construction**: Consider Production upgrade if HotM deployment critical
   - **Construction → Transition**: Evaluate Production profile for v0.2.0+ (public release)
