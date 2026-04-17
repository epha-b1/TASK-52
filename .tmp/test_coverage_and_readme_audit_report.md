# Combined Audit Report (Strict Mode)

## 1) Test Coverage Audit

### Project Type Detection
- README explicitly declares `fullstack` at top: `repo/README.md:3`.
- Repository structure confirms backend + frontend: `repo/src/backend/app.rs:54`, `repo/src/frontend/app.rs:18`.

### Backend Endpoint Inventory
Source: `repo/src/backend/app.rs:70-152`.

Total unique endpoints (`METHOD + PATH`): **69**

- Public: `/health`, `/auth/register`, `/auth/login`, `/traceability/verify/:code`
- Protected: auth/account, address-book, intake/inspections, media/evidence, supply, traceability, transfers, stock, members/checkin, profile, reports, audit
- Admin: users, config/version/rollback, diagnostics, jobs/logs, account purge, retention purge, key rotation

### API Test Mapping Table

Legend:
- **covered** = request to exact method+path found in tests
- **type** = true no-mock HTTP / HTTP with mocking / unit-only

| Endpoint | Covered | Type | Test Files | Evidence |
|---|---|---|---|---|
| GET `/health` | yes | true no-mock HTTP | API_tests/health_api_test.sh | `API_tests/health_api_test.sh:11` |
| POST `/auth/register` | yes | true no-mock HTTP | API_tests/auth_api_test.sh | `API_tests/auth_api_test.sh:13` |
| POST `/auth/login` | yes | true no-mock HTTP | API_tests/auth_api_test.sh | `API_tests/auth_api_test.sh:39` |
| GET `/traceability/verify/:code` | yes | true no-mock HTTP | API_tests/full_stack_test.sh | `API_tests/full_stack_test.sh:168` |
| POST `/auth/logout` | yes | true no-mock HTTP | API_tests/auth_api_test.sh | `API_tests/auth_api_test.sh:181` |
| GET `/auth/me` | yes | true no-mock HTTP | API_tests/auth_api_test.sh | `API_tests/auth_api_test.sh:59` |
| PATCH `/auth/change-password` | yes | true no-mock HTTP | API_tests/auth_api_test.sh | `API_tests/auth_api_test.sh:157` |
| POST `/account/delete` | yes | true no-mock HTTP | API_tests/blockers_api_test.sh | `API_tests/blockers_api_test.sh:130` |
| POST `/account/cancel-deletion` | yes | true no-mock HTTP | API_tests/blockers_api_test.sh | `API_tests/blockers_api_test.sh:134` |
| GET `/address-book` | yes | true no-mock HTTP | API_tests/address_book_api_test.sh | `API_tests/address_book_api_test.sh:39` |
| POST `/address-book` | yes | true no-mock HTTP | API_tests/address_book_api_test.sh | `API_tests/address_book_api_test.sh:13` |
| PATCH `/address-book/:id` | yes | true no-mock HTTP | API_tests/blockers_api_test.sh | `API_tests/blockers_api_test.sh:83` |
| DELETE `/address-book/:id` | yes | true no-mock HTTP | API_tests/address_book_api_test.sh | `API_tests/address_book_api_test.sh:60` |
| GET `/intake` | yes | true no-mock HTTP | API_tests/intake_api_test.sh | `API_tests/intake_api_test.sh:23` |
| POST `/intake` | yes | true no-mock HTTP | API_tests/remediation_api_test.sh | `API_tests/remediation_api_test.sh:51` |
| GET `/intake/:id` | yes | true no-mock HTTP | API_tests/intake_api_test.sh | `API_tests/intake_api_test.sh:30` |
| PATCH `/intake/:id/status` | yes | true no-mock HTTP | API_tests/intake_api_test.sh | `API_tests/intake_api_test.sh:37` |
| GET `/inspections` | yes | true no-mock HTTP | API_tests/coverage_gap_api_test.sh | `API_tests/coverage_gap_api_test.sh:92` |
| POST `/inspections` | yes | true no-mock HTTP | API_tests/intake_api_test.sh | `API_tests/intake_api_test.sh:51` |
| PATCH `/inspections/:id/resolve` | yes | true no-mock HTTP | API_tests/intake_api_test.sh | `API_tests/intake_api_test.sh:62` |
| POST `/media/upload/start` | yes | true no-mock HTTP | API_tests/full_stack_test.sh | `API_tests/full_stack_test.sh:78` |
| POST `/media/upload/chunk` | yes | true no-mock HTTP | API_tests/full_stack_test.sh | `API_tests/full_stack_test.sh:84` |
| POST `/media/upload/complete` | yes | true no-mock HTTP | API_tests/full_stack_test.sh | `API_tests/full_stack_test.sh:89` |
| GET `/evidence` | yes | true no-mock HTTP | API_tests/remediation_api_test.sh | `API_tests/remediation_api_test.sh:167` |
| DELETE `/evidence/:id` | yes | true no-mock HTTP | API_tests/full_stack_test.sh | `API_tests/full_stack_test.sh:113` |
| POST `/evidence/:id/link` | yes | true no-mock HTTP | API_tests/full_stack_test.sh | `API_tests/full_stack_test.sh:108` |
| PATCH `/evidence/:id/legal-hold` | yes | true no-mock HTTP | API_tests/blockers_api_test.sh | `API_tests/blockers_api_test.sh:381` |
| GET `/supply-entries` | yes | true no-mock HTTP | API_tests/remediation_regression_test.sh | `API_tests/remediation_regression_test.sh:178` |
| POST `/supply-entries` | yes | true no-mock HTTP | API_tests/full_stack_test.sh | `API_tests/full_stack_test.sh:123` |
| PATCH `/supply-entries/:id/resolve` | yes | true no-mock HTTP | API_tests/coverage_gap_api_test.sh | `API_tests/coverage_gap_api_test.sh:126` |
| GET `/traceability` | yes | true no-mock HTTP | API_tests/coverage_gap_api_test.sh | `API_tests/coverage_gap_api_test.sh:173` |
| POST `/traceability` | yes | true no-mock HTTP | API_tests/full_stack_test.sh | `API_tests/full_stack_test.sh:160` |
| POST `/traceability/:id/publish` | yes | true no-mock HTTP | API_tests/full_stack_test.sh | `API_tests/full_stack_test.sh:182` |
| POST `/traceability/:id/retract` | yes | true no-mock HTTP | API_tests/full_stack_test.sh | `API_tests/full_stack_test.sh:187` |
| GET `/traceability/:id/steps` | yes | true no-mock HTTP | API_tests/blockers_api_test.sh | `API_tests/blockers_api_test.sh:728` |
| POST `/traceability/:id/steps` | yes | true no-mock HTTP | API_tests/blockers_api_test.sh | `API_tests/blockers_api_test.sh:724` |
| GET `/transfers` | yes | true no-mock HTTP | API_tests/blockers_api_test.sh | `API_tests/blockers_api_test.sh:574` |
| POST `/transfers` | yes | true no-mock HTTP | API_tests/blockers_api_test.sh | `API_tests/blockers_api_test.sh:525` |
| GET `/transfers/:id` | yes | true no-mock HTTP | API_tests/coverage_gap_api_test.sh | `API_tests/coverage_gap_api_test.sh:207` |
| PATCH `/transfers/:id/status` | yes | true no-mock HTTP | API_tests/blockers_api_test.sh | `API_tests/blockers_api_test.sh:545` |
| GET `/stock/movements` | yes | true no-mock HTTP | API_tests/coverage_gap_api_test.sh | `API_tests/coverage_gap_api_test.sh:251` |
| POST `/stock/movements` | yes | true no-mock HTTP | API_tests/blockers_api_test.sh | `API_tests/blockers_api_test.sh:589` |
| GET `/stock/inventory` | yes | true no-mock HTTP | API_tests/blockers_api_test.sh | `API_tests/blockers_api_test.sh:584` |
| GET `/members` | yes | true no-mock HTTP | API_tests/remediation_regression_test.sh | `API_tests/remediation_regression_test.sh:211` |
| POST `/members` | yes | true no-mock HTTP | API_tests/full_stack_test.sh | `API_tests/full_stack_test.sh:198` |
| POST `/checkin` | yes | true no-mock HTTP | API_tests/full_stack_test.sh | `API_tests/full_stack_test.sh:203` |
| GET `/checkin/history` | yes | true no-mock HTTP | API_tests/remediation_regression_test.sh | `API_tests/remediation_regression_test.sh:210` |
| GET `/profile/privacy-preferences` | yes | true no-mock HTTP | API_tests/audit_fixes_test.sh | `API_tests/audit_fixes_test.sh:329` |
| PATCH `/profile/privacy-preferences` | yes | true no-mock HTTP | API_tests/audit_fixes_test.sh | `API_tests/audit_fixes_test.sh:340` |
| GET `/reports/summary` | yes | true no-mock HTTP | API_tests/full_stack_test.sh | `API_tests/full_stack_test.sh:229` |
| GET `/reports/export` | yes | true no-mock HTTP | API_tests/full_stack_test.sh | `API_tests/full_stack_test.sh:233` |
| GET `/reports/adoption-conversion` | yes | true no-mock HTTP | API_tests/full_stack_test.sh | `API_tests/full_stack_test.sh:241` |
| GET `/audit-logs` | yes | true no-mock HTTP | API_tests/full_stack_test.sh | `API_tests/full_stack_test.sh:282` |
| GET `/audit-logs/export` | yes | true no-mock HTTP | API_tests/full_stack_test.sh | `API_tests/full_stack_test.sh:290` |
| GET `/users` | yes | true no-mock HTTP | API_tests/auth_api_test.sh | `API_tests/auth_api_test.sh:116` |
| POST `/users` | yes | true no-mock HTTP | API_tests/auth_api_test.sh | `API_tests/auth_api_test.sh:92` |
| PATCH `/users/:id` | yes | true no-mock HTTP | API_tests/coverage_gap_api_test.sh | `API_tests/coverage_gap_api_test.sh:294` |
| DELETE `/users/:id` | yes | true no-mock HTTP | API_tests/remediation_regression_test.sh | `API_tests/remediation_regression_test.sh:331` |
| GET `/admin/config` | yes | true no-mock HTTP | API_tests/full_stack_test.sh | `API_tests/full_stack_test.sh:251` |
| PATCH `/admin/config` | yes | true no-mock HTTP | API_tests/full_stack_test.sh | `API_tests/full_stack_test.sh:259` |
| GET `/admin/config/versions` | yes | true no-mock HTTP | API_tests/full_stack_test.sh | `API_tests/full_stack_test.sh:264` |
| POST `/admin/config/rollback/:id` | yes | true no-mock HTTP | API_tests/blockers_api_test.sh | `API_tests/blockers_api_test.sh:207` |
| POST `/admin/diagnostics/export` | yes | true no-mock HTTP | API_tests/full_stack_test.sh | `API_tests/full_stack_test.sh:268` |
| GET `/admin/diagnostics/download/:id` | yes | true no-mock HTTP | API_tests/remediation_api_test.sh | `API_tests/remediation_api_test.sh:299` |
| GET `/admin/jobs` | yes | true no-mock HTTP | API_tests/full_stack_test.sh | `API_tests/full_stack_test.sh:272` |
| GET `/admin/logs` | yes | true no-mock HTTP | API_tests/blockers_api_test.sh | `API_tests/blockers_api_test.sh:284` |
| POST `/admin/account-purge` | yes | true no-mock HTTP | API_tests/blockers_api_test.sh | `API_tests/blockers_api_test.sh:143` |
| POST `/admin/retention-purge` | yes | true no-mock HTTP | API_tests/blockers_api_test.sh | `API_tests/blockers_api_test.sh:386` |
| POST `/admin/security/rotate-key` | yes | true no-mock HTTP | API_tests/remediation_api_test.sh | `API_tests/remediation_api_test.sh:322` |

### API Test Classification
1. **True No-Mock HTTP:** present and dominant (`curl`/`wget` to real routes on `http://localhost:8080`), e.g. `repo/API_tests/coverage_gap_api_test.sh:92`, `repo/API_tests/full_stack_test.sh:23-310`.
2. **HTTP with Mocking:** none detected.
3. **Non-HTTP unit/in-process tests:** backend Rust unit tests in modules like `repo/src/backend/crypto.rs:128`, `repo/src/backend/common.rs:192`, `repo/src/backend/modules/evidence/handlers.rs:919`.

### Mock Detection
- No `jest.mock`, `vi.mock`, `sinon.stub` occurrences in API test scripts.
- No transport/controller/service mocking evidence in API suites.
- API coverage evidence is direct HTTP calls through app server routes.

### Coverage Summary
- Total endpoints: **69**
- Endpoints with HTTP tests: **69**
- Endpoints with true no-mock HTTP tests: **69**
- HTTP coverage %: **100.0%**
- True API coverage %: **100.0%**

### Unit Test Summary

#### Backend Unit Tests
- Present in backend Rust modules:
  - `repo/src/backend/crypto.rs:128`
  - `repo/src/backend/common.rs:192`
  - `repo/src/backend/error.rs:80`
  - `repo/src/backend/zip.rs:166`
  - `repo/src/backend/config.rs:131`
  - `repo/src/backend/modules/evidence/handlers.rs:919`
  - `repo/src/backend/modules/supply/parser.rs:38`
  - `repo/src/backend/modules/traceability/code.rs:30`
  - `repo/src/backend/modules/transfers/handlers.rs:193`

- Important backend modules still lacking direct unit coverage evidence:
  - `repo/src/backend/modules/auth/handlers.rs`
  - `repo/src/backend/modules/users/handlers.rs`
  - `repo/src/backend/modules/admin/handlers.rs`
  - `repo/src/backend/modules/checkin/handlers.rs`
  - `repo/src/backend/modules/dashboard/handlers.rs`
  - `repo/src/backend/modules/intake/handlers.rs`
  - `repo/src/backend/modules/inspections/handlers.rs`
  - `repo/src/backend/modules/profile/handlers.rs`
  - `repo/src/backend/modules/stock/handlers.rs`

#### Frontend Unit Tests (Strict)
- Frontend test files (`*.test.*` / `*.spec.*`) in frontend scope: **NONE found**.
- Frontend test framework evidence: **NONE found**.
- Tests importing/rendering frontend components/modules: **NONE found**.
- Important frontend modules not unit-tested:
  - `repo/src/frontend/app.rs`
  - `repo/src/frontend/components/nav.rs`
  - `repo/src/frontend/pages/*` (all pages listed in `repo/src/frontend/pages/mod.rs:1-14`)
  - `repo/src/frontend/api/client.rs`
  - `repo/src/frontend/draft.rs`

**Mandatory verdict: Frontend unit tests: MISSING**

**Strict failure rule result:** project is fullstack and frontend unit tests are missing → **CRITICAL GAP**.

### Cross-Layer Observation
- Backend/API testing is extensive and now route-complete.
- Frontend unit testing remains absent; testing is still backend-heavy and unbalanced.

### API Observability Check
- Improved for prior weak endpoints via `coverage_gap_api_test.sh`, with explicit request+response assertions (fields, filters, role behavior), e.g. `repo/API_tests/coverage_gap_api_test.sh:95-101`, `repo/API_tests/coverage_gap_api_test.sh:253-275`, `repo/API_tests/coverage_gap_api_test.sh:299-314`.
- Legacy suites still include some status-focused checks, but overall API observability is now **moderate-to-strong**.

### Tests Check
- `run_tests.sh` runs tests inside Docker container(s) and orchestrates suites with DB reset: `repo/run_tests.sh:105-208`.
- Includes dedicated coverage-gap suite: `repo/run_tests.sh:206-208`.
- Docker-based testing requirement: **OK**.

### End-to-End Expectations (Fullstack)
- Expected: FE↔BE real-flow tests.
- Present: API-heavy integration suites + a frontend bundle/string integration script (`repo/API_tests/frontend_draft_test.sh`).
- Missing: browser-level frontend E2E automation evidence.

### Test Coverage Score (0-100)
**84 / 100**

### Score Rationale
- + 100% endpoint HTTP coverage with true route-level tests.
- + Broad negative/role/boundary tests across backend API.
- - Frontend unit tests missing in fullstack project (critical gap).
- - No browser-level FE↔BE E2E evidence.

### Key Gaps
1. **Critical:** frontend unit tests missing.
2. FE↔BE browser E2E testing not evidenced.

### Confidence & Assumptions
- Confidence: **high** for endpoint inventory and coverage mapping (static route + request evidence).
- Confidence: **medium-high** for quality/sufficiency assessment under static-only constraints.
- No runtime execution performed.

**Test Coverage Verdict: PARTIAL PASS (fails strict fullstack frontend-unit requirement)**

---

## 2) README Audit

### README Location
- Exists at required path: `repo/README.md`.

### Hard Gates

#### Formatting
- Markdown is structured and readable: headings, tables, code blocks present (`repo/README.md:1-486`).

#### Startup Instructions (Backend/Fullstack)
- Required literal `docker-compose up` present: `repo/README.md:67`.
- Alternative `docker compose up --build` also present: `repo/README.md:74`.
- **PASS**.

#### Access Method
- URL + port clearly defined: `repo/README.md:77-80`.
- **PASS**.

#### Verification Method
- API verification examples via curl are present: `repo/README.md:32-50`.
- Web/UI verification URL present: `repo/README.md:29`, `repo/README.md:80`.
- **PASS**.

#### Environment Rules (Docker-contained)
- README does not instruct runtime package installs (`npm/pip/apt/manual DB setup`).
- Docker-first instructions are used.
- **PASS**.

#### Demo Credentials (Auth conditional)
- Authentication exists and credentials are documented with roles:
  - table with admin/staff/auditor creds: `repo/README.md:23-27`
  - role identifiers stated: `repo/README.md:53`
- **PASS**.

### Engineering Quality
- Tech stack clarity: explicit backend/frontend/db stack (`repo/README.md:9-14`).
- Architecture and policy depth: role matrix, transfer lifecycle, stock ledger, dashboard filters, traceability timeline (`repo/README.md:135-233`).
- Test workflow documented: `repo/README.md:82-117`.
- Security/roles explained (password, lockout, session behavior, role matrix): `repo/README.md:55-57`, `repo/README.md:135-155`.

### High Priority Issues
- None.

### Medium Priority Issues
- None mandatory.

### Low Priority Issues
- README is long; an abridged quick-start section could improve scan speed (quality suggestion, not a compliance issue).

### Hard Gate Failures
- None.

### README Verdict
**PASS**

---

## Final Verdicts
- **Test Coverage Audit:** PARTIAL PASS (blocked by missing frontend unit tests under strict fullstack rule)
- **README Audit:** PASS
