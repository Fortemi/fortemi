# Option Matrix (Project Context & Intent)

**Purpose**: Capture what matric-memory IS - its nature, audience, constraints, and intent - to determine appropriate SDLC framework application and architectural approach.

**Generated**: 2026-01-02 (from Gitea issues #1, HotM #6, #7)

## Step 1: Project Reality

### What IS This Project?

**Project Description** (in natural language):

```
matric-memory is a Rust library crate providing vector-enhanced PostgreSQL storage, hybrid
search (FTS + semantic), and NLP pipeline management. Extracted from the HotM note-taking
application to enable reuse across multiple consumer applications and agents.

**Key Characteristics**:
- Library/crate (not standalone application)
- Migration/extraction project (proven code from HotM, not greenfield)
- Foundational infrastructure (enables semantic search for consumers)
- Solo developer with automation (roctinam + roctibot)
- v0.1.0 timeline: 6-8 weeks (20 issues in milestone)
- Internal tooling (no external users, no compliance requirements)
```

### Audience & Scale

**Who uses this?** (check all that apply):
- [ ] Just me (personal project)
- [ ] Small team (2-10 people, known individuals)
- [x] Department (10-100 people, organization-internal) - **Developer library consumers**
- [ ] External customers (100-10k users, paying or free)
- [ ] Large scale (10k-100k+ users, public-facing)
- [x] Other: **Library crate consumed by applications (HotM initially, agents future)**

**Audience Characteristics**:
- **Technical sophistication**: Technical (Rust developers integrating matric-memory)
- **User risk tolerance**: Expects stability (library dependency, breaking changes disruptive)
- **Support expectations**: Best-effort (solo dev, GitHub/Gitea issues, no formal SLA)

**Usage Scale** (from Gitea issues):
- **Active consumers**: 1 initially (HotM), 3-5 in 6 months (experimental agents), 10+ in 2 years
- **Document corpus**: 10k-100k documents (HotM notes), scaling to 100k-1M with multiple consumers
- **Request volume**: 10 searches/sec initially (single consumer), 100+ searches/sec with multiple consumers
- **Data volume**: 1-10 GB initially (embeddings for 10k docs), 10-100 GB at scale
- **Geographic distribution**: Single location (local/regional deployments by consumers)

### Deployment & Infrastructure

**Expected Deployment Model**:
- [ ] Static site (HTML/CSS/JS, no backend, GitHub Pages/Netlify/Vercel)
- [ ] Client-server (SPA + API backend, traditional web app)
- [x] **Library crate** (Rust dependency, embedded in consumer applications)
- [ ] Multi-system (microservices, service mesh, distributed)
- [ ] Serverless (AWS Lambda, Cloud Functions, event-driven)
- [ ] Mobile (iOS/Android native or React Native/Flutter)
- [ ] Desktop (Electron, native apps)
- [ ] CLI tool (command-line utility)
- [ ] Hybrid (multiple deployment patterns)

**Where does this run?**:
- [x] **Embedded in consumer applications** (HotM, agents)
- [x] **Local development** (developer workstation, Docker Compose for testing)
- [ ] Cloud platform (AWS, GCP, Azure, Vercel, Netlify)
- [ ] On-premise (company servers, data center)
- [ ] Hybrid (cloud + on-premise)

**Infrastructure Complexity**:
- **Deployment type**: Library crate (no deployment, consumed as Rust dependency)
- **Data persistence**: PostgreSQL database (managed by consumer, matric-memory provides schema)
- **External dependencies**:
  1. PostgreSQL 14+ (required, with pgvector extension)
  2. Ollama (optional, for local inference backend)
  3. OpenAI API (optional, for cloud inference backend)
- **Network topology**: Embedded in consumer application (single process or multi-tier depending on consumer)

### Technical Complexity

**Codebase Characteristics** (estimated):
- **Size**: 5k-10k LoC (migrating from HotM, adding abstractions and packaging)
- **Languages**: Rust (primary), SQL (schema, migrations)
- **Architecture**: Multi-crate workspace (matric-core, matric-db, matric-search, matric-inference, matric-jobs, matric-datasets)
- **Team familiarity**: Extracting existing HotM code (familiar domain, new project structure)

**Technical Risk Factors** (check all detected):
- [x] **Performance-sensitive** (search latency <200ms critical, p95 target)
- [x] **Security-sensitive** (documents may contain PII, proprietary research, need encryption)
- [ ] Data integrity-critical (financial, medical, legal records) - not critical (notes/research)
- [x] **High concurrency** (multiple consumers, async search/embedding operations)
- [x] **Complex business logic** (hybrid search ranking, inference routing, job scheduling)
- [x] **Integration-heavy** (PostgreSQL, pgvector, Ollama, OpenAI API, consumer applications)
- [ ] None (straightforward technical requirements)

---

## Step 2: Constraints & Context

### Resources

**Team** (from Gitea):
- **Size**: 1 developer (roctinam) + automation (roctibot for issue management)
- **Experience**: Senior Rust (HotM codebase author), Intermediate PostgreSQL/embeddings, Beginner pgvector
- **Availability**: Part-time (side project, 6-8 week timeline suggests 10-15 hours/week)

**Budget**:
- **Development**: Volunteer (no budget, time-constrained)
- **Infrastructure**: Free tier/local (PostgreSQL local, Ollama local, no cloud costs)
- **Timeline**: 6-8 weeks to v0.1.0 (20 issues in milestone)

### Regulatory & Compliance

**Data Sensitivity** (check all):
- [ ] Public data only (no privacy concerns)
- [x] **User-provided content** (notes, research, documents)
- [x] **Personally Identifiable Information** (PII: names, research subjects, personal notes)
- [ ] Payment information (credit cards, financial accounts)
- [ ] Protected Health Information (PHI: medical records)
- [x] **Sensitive business data** (proprietary research, confidential notes)

**Regulatory Requirements**:
- [x] **None** (internal tooling, no regulatory requirements)
- [ ] GDPR (EU users, data privacy)
- [ ] CCPA (California users)
- [ ] HIPAA (US healthcare)
- [ ] PCI-DSS (payment card processing)
- [ ] SOX (US financial reporting)
- [ ] SOC2 (service organization controls)

**Contractual Obligations**:
- [x] **None** (no contracts, no SLA commitments)
- [ ] SLA commitments (uptime, response time guarantees)
- [ ] Security requirements (penetration testing, audits)
- [ ] Compliance certifications (SOC2, ISO27001)

### Technical Context

**Current State** (for new project, extracted from HotM):
- **Current stage**: Planning/Inception (new repository, migrating code from HotM)
- **Existing code**: HotM backend API (db.rs, models.rs, ollama.rs, routes/search.rs, routes/semantic.rs, job_queue.rs)
- **Test coverage**: HotM has minimal tests (<20%), target 60% for matric-memory
- **Documentation**: HotM has basic README, matric-memory needs comprehensive API docs
- **Deployment automation**: HotM uses Docker, matric-memory needs CI/CD (Gitea Actions)

---

## Step 3: Priorities & Trade-offs

### What Matters Most?

**Rank these priorities** (1 = most important, 4 = least important):

From project analysis:
- **2** - Speed to delivery (6-8 week timeline, but extracting existing code reduces greenfield risk)
- **1** - Cost efficiency (solo dev, volunteer time, minimize complexity and infrastructure)
- **3** - Quality & security (library crate, consumers depend on stability, but MVP acceptable)
- **4** - Reliability & scale (99% uptime sufficient, internal tooling, 10k-100k documents)

**Priority Weights** (must sum to 1.0):

| Criterion | Weight | Rationale |
|-----------|--------|-----------|
| **Delivery speed** | 0.25 | 6-8 week timeline tight, but extracting proven code (not greenfield) reduces risk. Moderate urgency. |
| **Cost efficiency** | 0.35 | Solo developer, volunteer time. Minimize infrastructure, tooling, and complexity. **Highest priority**. |
| **Quality/security** | 0.30 | Library crate has higher quality bar (consumers depend on stability), but MVP quality acceptable. Dataset encryption important. |
| **Reliability/scale** | 0.10 | 99% uptime sufficient (internal), 10k-100k documents achievable with PostgreSQL. Low priority for v0.1.0. |
| **TOTAL** | **1.00** | ← Must sum to 1.0 |

**Rationale**:
- **Cost efficiency highest (0.35)**: Solo dev with limited time must minimize complexity. No budget for cloud, managed services, or complex tooling.
- **Quality second (0.30)**: Library crate needs stability and docs (consumers integrate deeply), but MVP quality (60% tests) acceptable.
- **Speed third (0.25)**: 6-8 weeks is tight, but extracting existing HotM code reduces greenfield risk. Not urgent (no external deadline).
- **Reliability lowest (0.10)**: Internal tooling, 99% uptime sufficient, no SLA. PostgreSQL handles 10k-100k docs without exotic infrastructure.

### Trade-off Context

**What are you optimizing for?**
```
Minimize infrastructure complexity and development overhead while delivering
a stable, well-documented library crate for HotM integration.

**Key optimizations**:
1. **Reuse PostgreSQL** (already in HotM) rather than adding dedicated vector DB (Qdrant/Weaviate)
2. **Extract existing code** (HotM migration) rather than greenfield rewrite
3. **Simple job queue** (PostgreSQL-backed) rather than Redis/RabbitMQ
4. **Local inference** (Ollama) with pluggable abstraction (low-cost flexibility)
5. **Comprehensive docs** (API reference, integration guide) to minimize consumer support burden
```

**What are you willing to sacrifice?**
```
1. **Scale beyond 100k documents**: PostgreSQL+pgvector sufficient for v0.1.0, can upgrade to Qdrant if needed post-MVP
2. **Real-time performance**: p95 <200ms acceptable, not <50ms (lower priority than simplicity)
3. **Multi-tenancy**: Single-user/single-app focus (HotM), defer tenant isolation to v0.2.0+
4. **Advanced inference backends**: Ollama + OpenAI sufficient, defer Hugging Face/Anthropic to v0.2.0
5. **Edge case test coverage**: 60% coverage acceptable for v0.1.0, defer to 70-80% post-MVP
```

**What is non-negotiable?**
```
1. **Dataset encryption**: Must support AES-256 encryption for .matric exports (sensitive data)
2. **Pluggable inference**: InferenceBackend trait abstraction (future-proofs for multiple backends)
3. **API stability**: Public API must be well-designed (breaking changes disruptive for consumers)
4. **Comprehensive docs**: API reference, integration guide, examples (library adoption depends on docs)
5. **HotM integration success**: v0.1.0 release gated on HotM successfully consuming matric-memory
```

---

## Step 4: Intent & Decision Context

### Why This Intake Now?

**What triggered this intake?**:
- [x] **Starting new project** (extracting matric-memory from HotM)
- [x] **Seeking SDLC structure** (want organized process for library development)
- [x] **Team alignment** (define API boundaries between matric-memory and HotM) - HotM issue #7
- [ ] Funding/business milestone (investor pitch, customer demo)

**What decisions need making?**:
```
1. **Crate structure**: Monolithic vs multi-crate workspace?
   - Context: Extracting 6 components from HotM (db, search, inference, jobs, datasets, core)
   - Decision: Multi-crate workspace chosen (modularity, independent compilation)

2. **Database choice**: PostgreSQL+pgvector vs dedicated vector DB (Qdrant, Weaviate)?
   - Context: HotM already uses PostgreSQL, but pgvector performance uncertain at scale
   - Decision: PostgreSQL+pgvector chosen (cost efficiency, simplicity, proven for 100k docs)

3. **Inference abstraction**: Tight Ollama coupling vs pluggable backends?
   - Context: HotM uses Ollama, but future consumers may want OpenAI, Anthropic, Hugging Face
   - Decision: InferenceBackend trait chosen (flexibility, future-proof)

4. **Job queue**: PostgreSQL-backed vs Redis vs In-memory?
   - Context: Need async embedding processing, but avoid infrastructure complexity
   - Decision: PostgreSQL-backed chosen initially (simplicity), Redis if scale requires

5. **Packaging format**: Custom .matric vs standard (tar.gz, zip)?
   - Context: Need encryption, metadata, versioning for datasets
   - Decision: Custom .matric format chosen (control, encryption, extensibility)
```

**What's uncertain or controversial?**:
```
1. **pgvector performance at 100k+ documents**: Uncertain if HNSW index scales sufficiently
   - Mitigation: Benchmark early (issue #4), document limits, fallback to Qdrant if needed

2. **API boundary between matric-memory and HotM**: Unclear which functions should be public vs internal
   - Mitigation: Issue #2 (Define Public API Surface), start minimal, expand based on HotM needs

3. **Solo developer capacity for 20 issues in 6-8 weeks**: Risk of timeline slip
   - Mitigation: Ruthless prioritization, defer non-critical issues to v0.2.0
```

**Success criteria for this intake process**:
```
1. **Clear architectural direction**: Crate structure, database choice, abstraction layers defined
2. **Stakeholder alignment**: HotM integration plan validated (issues #6, #7)
3. **Realistic timeline and scope**: 20 issues triaged (critical vs deferred)
4. **Ready to start Inception**: Architecture decisions documented (ADRs), ready to code
```

---

## Step 5: Framework Application

### Relevant SDLC Components

Based on project reality (library crate, solo dev, MVP profile), which framework components are relevant?

**Templates** (check applicable):
- [x] **Intake** (project-intake, solution-profile, option-matrix) - Always include
- [x] **Requirements** (user-stories, API contracts) - Include for public API definition (issue #2)
  - Use Gitea issues (#1-#20) as requirements backlog
  - Document API contracts (trait definitions, public interfaces)
  - Skip heavyweight use-cases (library, not application)
- [x] **Architecture** (ADRs, API contracts) - Include for key decisions
  - ADR-001: Workspace structure
  - ADR-002: PostgreSQL+pgvector choice
  - ADR-003: InferenceBackend trait abstraction
  - ADR-004: .matric package format
  - Skip comprehensive SAD (README architecture section + ADRs sufficient)
- [x] **Test** (test-strategy in README) - Include lightweight strategy
  - Document 60% coverage target
  - Unit tests for all public APIs
  - Integration tests with PostgreSQL/Ollama
  - Skip formal test-plan (inline in README or CONTRIBUTING.md)
- [x] **Security** (dependency scanning, encryption design) - Include lightweight security
  - cargo audit in CI (dependency vulnerabilities)
  - Dataset encryption design (AES-256, key management)
  - Skip threat-model (library, no network attack surface)
- [ ] **Deployment** (deployment-plan, runbook, ORR) - Skip (library crate, no deployment)
  - Consumers handle deployment
  - Document PostgreSQL setup in README
- [ ] **Governance** (decision-log, CCB-minutes, RACI) - Skip (solo dev, informal)

**Commands** (check applicable):
- [x] **Intake commands** (/intake-wizard, /intake-start) - Always include
- [x] **Flow commands** (/flow-inception-to-elaboration, /flow-gate-check) - Include for phase transitions
  - /flow-concept-to-inception (start Inception)
  - /flow-inception-to-elaboration (after architecture solidifies)
  - /flow-gate-check (validate v0.1.0 readiness)
- [x] **Quality gates** (/security-gate) - Include lightweight security validation
  - /security-gate (check cargo audit, dataset encryption design)
- [x] **Specialized** (/pr-review, /create-prd) - Include as needed
  - /pr-review (code quality, self-review assistant)
  - /generate-tests (test generation for public APIs)

**Agents** (check applicable):
- [x] **Core SDLC agents** (architecture-designer, requirements-analyst, test-engineer, code-reviewer) - Include
  - Architecture Designer (crate structure, trait design, ADRs)
  - Requirements Analyst (refine Gitea issues, define API contracts)
  - Test Engineer (integration test strategy, 60% coverage plan)
  - Code Reviewer (self-review assistant, clippy enhancements)
  - Technical Writer (API docs, integration guide, examples)
- [x] **Security specialists** (security-auditor) - Include lightweight security
  - Security Auditor (dependency audit, encryption design review)
  - Skip Security Gatekeeper (no compliance, no threat model needed)
- [ ] **Operations specialists** (incident-responder, reliability-engineer) - Skip (library crate, no ops)
- [ ] **Enterprise specialists** (legal-liaison, compliance-validator, privacy-officer) - Skip (no compliance)

**Process Rigor Level**:
- [ ] Minimal (README, lightweight notes) - For: Prototype (solo, <4 weeks, experimental)
- [x] **Moderate** (user stories, basic architecture, test plan) - For: **MVP** (small team, 1-3 months, proving viability) ✓
  - **Enhanced documentation**: ADRs, API docs, integration guide (library crate needs quality docs)
  - **Lightweight process**: No heavyweight governance, no formal requirements docs (use Gitea issues)
  - **Solo dev tailoring**: Self-review, automation (clippy, cargo audit), comprehensive tests
- [ ] Full (comprehensive docs, traceability, gates) - For: Production (established users, compliance, mission-critical)
- [ ] Enterprise (audit trails, compliance evidence, change control) - For: Enterprise (contracts, >10k users, regulated)

### Rationale for Framework Choices

**Why this subset of framework?**:
```
matric-memory is an MVP library crate (6-8 weeks, solo dev, extracting proven code) requiring
Moderate rigor with enhanced documentation:

**Included**:
1. **Intake** (project-intake, solution-profile, option-matrix) - Foundation for all projects
2. **Requirements** (Gitea issues as backlog, API contracts for public surface) - Lightweight, issue-driven
3. **Architecture** (ADRs for key decisions, README for overview) - Captures rationale without heavyweight SAD
4. **Test** (60% coverage target, integration test strategy) - Higher bar for library crate
5. **Security** (cargo audit, dataset encryption) - Baseline+ posture (sensitive documents)
6. **Core agents** (architecture, requirements, test, code-review, tech-writer) - Solo dev support
7. **Flow commands** (phase transitions, gate checks) - Structured progression through SDLC

**Excluded** (and why):
1. **Deployment templates** - Library crate, no deployment (consumers handle)
2. **Governance docs** - Solo dev, informal process (no CCB, no change control)
3. **Comprehensive SAD** - ADRs + README architecture section sufficient for library
4. **Threat model** - Library crate, no network attack surface (consumers handle security)
5. **Operations agents** - No deployment, no incident response (library, not service)
6. **Enterprise specialists** - No compliance, no contracts, no audit requirements

**Tailoring for solo dev + library crate**:
- Use Gitea issues (#1-#20) instead of formal requirements docs
- ADRs instead of comprehensive SAD (lightweight, decision-focused)
- Self-review with automation (clippy, cargo audit) instead of formal PR reviews
- Enhanced docs (API, integration guide, examples) critical for library adoption
```

**What we're skipping and why**:
```
**Skipping Deployment/Operations templates because**:
- Library crate has no deployment (consumed as Rust dependency)
- Consumers (HotM, agents) handle their own deployment and operations
- matric-memory provides schema, consumers manage PostgreSQL

**Skipping Governance/Enterprise templates because**:
- Solo developer, no coordination overhead (no CCB, change control, RACI)
- No compliance requirements (internal tooling, no HIPAA/SOC2/GDPR)
- No contracts or SLAs (internal library, best-effort support)
- Informal process (Gitea issues, no heavyweight ceremonies)

**Skipping Comprehensive SAD because**:
- ADRs capture key architectural decisions (workspace, database, inference, packaging)
- README architecture section provides overview and diagrams
- Library crate needs API docs (rustdoc), not infrastructure diagrams
- Solo dev can maintain architecture in memory (ADRs for traceability)

**Skipping Threat Model because**:
- Library crate has no network-facing attack surface
- Consumers (HotM, agents) handle authentication, authorization, network security
- Dataset encryption addresses primary security concern (data at rest)
- Dependency scanning (cargo audit) covers supply chain risk

**Will revisit if**:
- **Multi-contributor**: Add CONTRIBUTING.md, governance, formal PR reviews
- **Public release (crates.io)**: Add comprehensive docs, formal versioning, breaking change policy
- **Compliance requirements**: Add threat model, security controls, audit evidence
- **Production profile**: Add deployment docs (if consumers need reference), runbooks, monitoring guides
```

---

## Step 6: Evolution & Adaptation

### Expected Changes

**How might this project evolve?**:
- [x] **User base growth**: 1 consumer (HotM) → 3-5 (agents) within 6 months, 10+ within 2 years
- [x] **Feature expansion**: v0.1.0 (core) → v0.2.0 (datasets, multi-backend) → v0.3.0+ (advanced features)
- [x] **Team expansion**: Solo dev initially, potential contributors if open-sourced (2 years)
- [x] **Commercial/monetization**: Possible (if library proves valuable, could offer hosted variant or support)
- [ ] Compliance requirements: None expected (internal tooling)
- [x] **Technical pivot**: Possible (if pgvector insufficient, migrate to Qdrant/Weaviate in v0.2.0+)

**Adaptation Triggers** (when to revisit framework application):
```
**Add Security templates when**:
- Multi-tenant use cases emerge (collection-level isolation, access control)
- Compliance requirements added (GDPR, HIPAA, SOC2)
- Public dataset sharing (need threat model for data leakage)
- **Trigger**: 5+ consumers, or first enterprise consumer request

**Add Governance templates when**:
- Team expands beyond solo dev (2+ contributors)
- Open-source release (community contributions, formal PR process)
- Breaking change policy needed (semantic versioning, deprecation timeline)
- **Trigger**: First external contributor, or crates.io publication

**Upgrade to Production profile when**:
- 5+ active consumers (stability critical, breaking changes disruptive)
- Public crates.io release (broader adoption, quality expectations higher)
- 100k+ document corpus (performance critical, need optimization)
- **Trigger**: v0.2.0 release, or 5+ consumers, or public crates.io

**Add Deployment templates when**:
- Hosted variant offered (matric-memory-as-a-service, managed PostgreSQL+Ollama)
- Reference deployment architectures requested (consumers need examples)
- Docker Compose or Kubernetes examples needed
- **Trigger**: First hosted deployment, or 3+ consumer requests for examples
```

**Planned Framework Evolution**:
- **Current (Inception, v0.1.0)**: Moderate rigor, enhanced docs, solo dev, MVP profile
  - Intake, ADRs, API docs, integration guide, test strategy, security basics
  - Core agents: architecture, requirements, test, code-review, tech-writer

- **3 months (Elaboration, v0.2.0)**: Add dataset packaging, multi-backend inference
  - Add: Dataset encryption design doc, performance benchmarks, examples
  - Maintain: Moderate rigor, solo dev (unless contributors join)

- **6 months (Construction, v0.3.0+)**: Consider Production upgrade if 5+ consumers
  - Add: Governance docs (if contributors), deployment examples (if requested)
  - Upgrade: Production profile if public crates.io release or enterprise consumers
  - Add: Comprehensive SAD (if multi-contributor), formal PR process

- **12 months (Transition, v1.0.0)**: Public release, stabilize API, production-ready
  - Add: Governance (CONTRIBUTING.md, CODE_OF_CONDUCT.md), formal versioning policy
  - Upgrade: Production profile (70-80% test coverage, comprehensive docs, SLO tracking)
  - Add: Deployment examples, hosted variant (if demand exists)

---

## Architectural Options Analysis

### Option A: Multi-Crate Workspace (PostgreSQL + pgvector)

**Description**: Multi-crate Cargo workspace with PostgreSQL+pgvector for vector storage, pluggable inference backends (Ollama, OpenAI), PostgreSQL-backed job queue, and custom .matric dataset format.

**Technology Stack**:
- **Crates**: matric-core (traits), matric-db (PostgreSQL+pgvector), matric-search (hybrid), matric-inference (Ollama+OpenAI), matric-jobs (PostgreSQL queue), matric-datasets (.matric format)
- **Database**: PostgreSQL 14+ with pgvector extension (HNSW index for similarity search)
- **Inference**: Ollama (local), OpenAI-compatible API (cloud), InferenceBackend trait abstraction
- **Job Queue**: PostgreSQL-backed (simple, no Redis dependency)
- **Packaging**: Custom .matric format (bincode serialization, AES-256 encryption)

**Scoring** (0-5 scale):

| Criterion | Score | Rationale |
|-----------|------:|-----------|
| Delivery Speed | 4/5 | Extracting existing HotM code (proven), multi-crate structure clear. Deduction: PostgreSQL+pgvector new, needs learning. |
| Cost Efficiency | 5/5 | **Zero infrastructure cost** (local PostgreSQL, local Ollama, no cloud). Solo dev can manage complexity. **Perfect score**. |
| Quality/Security | 4/5 | Strong foundation (Rust safety, PostgreSQL reliability, dataset encryption). Deduction: pgvector performance unproven at scale. |
| Reliability/Scale | 3/5 | PostgreSQL handles 10k-100k docs. Deduction: Uncertain beyond 100k (HNSW index tuning), single-instance bottleneck. |
| **Weighted Total** | **4.05/5.0** | (4×0.25) + (5×0.35) + (4×0.30) + (3×0.10) = 1.0 + 1.75 + 1.2 + 0.3 = **4.05** |

**Trade-offs**:
- **Pros**:
  - **Lowest cost** (zero infrastructure, reuse PostgreSQL from HotM)
  - **Simplest deployment** (single database, no additional services)
  - **Fast migration** (HotM already uses PostgreSQL)
  - **Pluggable inference** (InferenceBackend trait supports multiple backends)
  - **Proven database** (PostgreSQL stability, pgvector mature for 100k docs)
- **Cons**:
  - **Scale uncertainty** (pgvector HNSW index performance >100k docs unknown)
  - **Single-instance limit** (PostgreSQL not distributed, read replicas help but not sharding)
  - **Learning curve** (pgvector new, HNSW index tuning required)

**When to choose**: Solo dev, cost-constrained, 10k-100k document corpus, prefer simplicity over exotic performance.

---

### Option B: Multi-Crate Workspace (Qdrant or Weaviate Vector DB)

**Description**: Multi-crate Cargo workspace with dedicated vector database (Qdrant or Weaviate) for embeddings, PostgreSQL for metadata/FTS, pluggable inference, and custom .matric format.

**Technology Stack**:
- **Crates**: matric-core, matric-db (PostgreSQL metadata + Qdrant/Weaviate vectors), matric-search (hybrid), matric-inference (Ollama+OpenAI), matric-jobs (PostgreSQL queue), matric-datasets (.matric)
- **Database**: PostgreSQL (metadata, FTS), Qdrant or Weaviate (vector similarity)
- **Inference**: Ollama, OpenAI-compatible, InferenceBackend trait
- **Job Queue**: PostgreSQL-backed or Redis (if needed for scale)
- **Packaging**: Custom .matric format

**Scoring** (0-5 scale):

| Criterion | Score | Rationale |
|-----------|------:|-----------|
| Delivery Speed | 2/5 | **Slower migration** (need to integrate Qdrant/Weaviate, not in HotM). Learning curve. Two databases to manage. |
| Cost Efficiency | 2/5 | **Higher cost** (two databases to run, Qdrant/Weaviate container + PostgreSQL). More infrastructure complexity. |
| Quality/Security | 4/5 | Strong quality (proven vector DBs, optimized for scale). Same security as Option A (Rust, encryption). |
| Reliability/Scale | 5/5 | **Best scale** (Qdrant/Weaviate handle 1M+ docs, distributed, purpose-built). Proven for large corpora. |
| **Weighted Total** | **2.95/5.0** | (2×0.25) + (2×0.35) + (4×0.30) + (5×0.10) = 0.5 + 0.7 + 1.2 + 0.5 = **2.95** |

**Trade-offs**:
- **Pros**:
  - **Best scale** (Qdrant/Weaviate handle 1M+ docs, distributed)
  - **Optimized performance** (purpose-built vector search, HNSW tuning mature)
  - **Future-proof** (no PostgreSQL scaling limits, horizontal scaling)
- **Cons**:
  - **Higher cost** (two databases, more infrastructure to manage)
  - **Slower migration** (Qdrant/Weaviate new, not in HotM)
  - **Complexity** (two databases, two connection pools, sync metadata↔vectors)
  - **Over-engineered for MVP** (10k-100k docs don't need dedicated vector DB)

**When to choose**: Large scale (>100k docs), budget for infrastructure, performance-critical, distributed deployment needed.

---

### Option C: Monolithic Crate (PostgreSQL + pgvector, Tight Coupling)

**Description**: Single monolithic crate with all components tightly integrated, PostgreSQL+pgvector for storage, Ollama tightly coupled (no abstraction), no .matric packaging initially.

**Technology Stack**:
- **Crate**: Single `matric-memory` crate (all modules internal)
- **Database**: PostgreSQL 14+ with pgvector
- **Inference**: Ollama only (tight coupling, no trait abstraction)
- **Job Queue**: In-memory or PostgreSQL-backed (simple)
- **Packaging**: Deferred to v0.2.0 (not in scope for monolith MVP)

**Scoring** (0-5 scale):

| Criterion | Score | Rationale |
|-----------|------:|-----------|
| Delivery Speed | 5/5 | **Fastest migration** (minimal refactoring, HotM code copied as-is). No crate structure overhead. Single compilation unit. |
| Cost Efficiency | 5/5 | **Zero infrastructure cost** (same as Option A). Simplest possible architecture. |
| Quality/Security | 2/5 | **Poor modularity** (tight coupling, hard to test components in isolation). No pluggable inference (locked to Ollama). |
| Reliability/Scale | 3/5 | Same scale as Option A (PostgreSQL+pgvector), but harder to optimize (monolithic, no separation). |
| **Weighted Total** | **3.75/5.0** | (5×0.25) + (5×0.35) + (2×0.30) + (3×0.10) = 1.25 + 1.75 + 0.6 + 0.3 = **3.75** |

**Trade-offs**:
- **Pros**:
  - **Fastest delivery** (minimal refactoring, copy HotM code, ship)
  - **Simplest architecture** (single crate, single compilation, no workspace complexity)
  - **Zero cost** (same as Option A, local PostgreSQL+Ollama)
- **Cons**:
  - **Poor quality** (tight coupling, hard to test, no modularity)
  - **No extensibility** (Ollama locked in, can't swap inference backends)
  - **Technical debt** (defeats purpose of extraction - need refactoring later)
  - **Consumer limitations** (can't selectively depend on subsets, e.g., db without search)

**When to choose**: Throwaway prototype, extreme time pressure, willing to refactor later. **Not recommended for matric-memory** (extraction goal is modularity and reusability).

---

## Recommendation

**Recommended Option**: **Option A - Multi-Crate Workspace (PostgreSQL + pgvector)** (Score: 4.05/5.0)

**Rationale**:

1. **Cost efficiency priority (0.35 weight)**: Option A has **zero infrastructure cost** (perfect score 5/5), highest priority met. Option B requires two databases (higher cost).

2. **Quality priority (0.30 weight)**: Option A has **strong quality** (4/5) with modular crate structure, pluggable inference, and dataset encryption. Option C fails quality (2/5, monolithic).

3. **Delivery speed (0.25 weight)**: Option A is **fast** (4/5) extracting HotM code with clear crate structure. Option B is slower (2/5, new vector DB). Option C is fastest (5/5) but sacrifices quality.

4. **Reliability/scale (0.10 weight)**: Option A is **sufficient** (3/5) for 10k-100k docs (MVP target). Option B is best (5/5) but over-engineered for v0.1.0.

**Key Decision Factors**:

- **PostgreSQL+pgvector proven for 100k docs**: Community reports (Hacker News, Reddit) confirm HNSW index performs well at this scale. MVP target is 10k-100k docs, well within proven range.

- **Modularity non-negotiable**: Purpose of extraction is reusability and extensibility. Monolithic (Option C) defeats the goal.

- **Qdrant/Weaviate over-engineered**: v0.1.0 targets 10k-100k docs, no need for distributed vector DB. Can migrate if scale exceeds 100k.

- **Solo dev capacity**: Multi-crate workspace (Option A) is manageable for experienced Rust dev, provides clear separation of concerns for testing.

**Sensitivities**:

- **If timeline pressure increases** (e.g., 4 weeks not 6-8 weeks):
  - Consider deferring dataset packaging (#12-16) to v0.2.0
  - Focus on core extraction (db, search, inference, jobs) only
  - **Do not** downgrade to Option C (monolith) - technical debt not worth speed

- **If scale projections exceed 100k documents**:
  - Re-benchmark pgvector HNSW index at 100k, 500k, 1M docs
  - If p95 latency >500ms, consider Option B (Qdrant/Weaviate migration)
  - Gradual migration: Keep PostgreSQL for metadata/FTS, add Qdrant for vectors only

- **If infrastructure budget available** (e.g., cloud hosting, managed services):
  - Consider Option B for future-proofing (Qdrant cloud managed, no infrastructure overhead)
  - Weigh cost vs. performance benefit (likely still favor Option A for v0.1.0, defer to v0.2.0)

**Implementation Plan**:

**Week 1-2** (Foundation):
1. Create Cargo workspace structure (#6): matric-core, matric-db, matric-search, matric-inference, matric-jobs, matric-datasets
2. Migrate database layer (#3): PostgreSQL connection pooling, pgvector integration, schema
3. Set up CI/CD (#20): Gitea Actions, lint (clippy/rustfmt), test, cargo audit
4. Write ADR-001 (workspace structure), ADR-002 (PostgreSQL+pgvector choice)

**Week 3-4** (Core Features):
1. Implement search engine (#4): FTS, semantic (pgvector), hybrid
2. Define InferenceBackend trait (#8), implement Ollama backend (#9)
3. Implement job queue (#5): PostgreSQL-backed, async processing
4. Define public API surface (#2): Core traits, data models, error handling

**Week 5-6** (Integration & Packaging):
1. Design .matric format (#13), implement export (#14), encryption (#15), import (#16)
2. Implement OpenAI-compatible backend (#10), inference routing (#11)
3. Integrate matric-memory into HotM (HotM #6): Update dependencies, test workflows

**Week 7-8** (Documentation & Validation):
1. Write README (#17), API docs (#18), integration guide (#19)
2. Write ADR-003 (InferenceBackend abstraction), ADR-004 (.matric format)
3. End-to-end testing (HotM integration, full pipeline)
4. Performance benchmarks (10k docs, 100k docs)
5. v0.1.0 release (tag, publish to Gitea, update HotM)

**Risks and Mitigations**:

**Risk 1: pgvector performance insufficient at 100k docs**
- **Mitigation**: Benchmark early (week 3-4), document HNSW index tuning parameters
- **Fallback**: If p95 >500ms, plan Qdrant migration for v0.2.0 (PostgreSQL metadata + Qdrant vectors)
- **Likelihood**: Low (community reports confirm 100k-500k docs viable with tuning)

**Risk 2: Solo dev capacity for 20 issues in 6-8 weeks**
- **Mitigation**: Ruthlessly prioritize, defer dataset packaging (#12-16) if timeline slips
- **Fallback**: Release v0.1.0 with core only (db, search, inference, jobs), v0.1.1 adds packaging
- **Likelihood**: Medium (20 issues ambitious, but extracting proven code reduces greenfield risk)

**Risk 3: API boundary instability (breaking changes post-v0.1.0)**
- **Mitigation**: Issue #2 (Define Public API Surface) early, minimize public surface, feature flags for experimental APIs
- **Fallback**: Semantic versioning (0.1.x patches, 0.2.0 minor breaking), deprecation warnings, migration guides
- **Likelihood**: Medium (library crate, breaking changes disruptive, but MVP acceptable)

---

## Next Steps

1. **Review option-matrix** and validate that Option A (multi-crate + PostgreSQL+pgvector) aligns with priorities
2. **Confirm architectural decisions** with HotM team (API boundaries from HotM #7)
3. **Start Inception phase** with recommended framework components:
   - Architecture Designer: Create ADR-001 (workspace), ADR-002 (database), ADR-003 (inference), ADR-004 (packaging)
   - Requirements Analyst: Refine Gitea issues #1-#20, define API contracts (InferenceBackend trait, etc.)
   - Test Engineer: Plan integration test strategy (PostgreSQL+pgvector, Ollama, job queue)
4. **Proceed to implementation** (Week 1-2 foundation work)
5. **Revisit framework selection** at phase gates:
   - **Inception → Elaboration**: After ADRs written, architecture validated
   - **Elaboration → Construction**: After pgvector benchmarks, confirm PostgreSQL sufficient
   - **Construction → Transition**: After HotM integration, validate v0.1.0 readiness
