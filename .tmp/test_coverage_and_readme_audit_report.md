# Unified Audit Report: Test Coverage + README (Strict Mode)

## Project Type Detection
- README top section does **not** explicitly declare one of: backend/fullstack/web/android/ios/desktop (`repo/README.md:1-5`).
- Light repository inspection shows both backend and frontend code (`repo/src/backend/app.rs:54`, `repo/src/frontend/app.rs:18`, `repo/src/frontend/pages/mod.rs:1`).
- **Inferred project type: fullstack.**

---

## 1) Test Coverage Audit

### Backend Endpoint Inventory
Source of truth: router registration in `repo/src/backend/app.rs:70-152`.

Total endpoints found: **69**

1. GET `/health`
2. POST `/auth/register`
3. POST `/auth/login`
4. GET `/traceability/verify/:code`
5. POST `/auth/logout`
6. GET `/auth/me`
7. PATCH `/auth/change-password`
8. POST `/account/delete`
9. POST `/account/cancel-deletion`
10. GET `/address-book`
11. POST `/address-book`
12. PATCH `/address-book/:id`
13. DELETE `/address-book/:id`
14. GET `/intake`
15. POST `/intake`
16. GET `/intake/:id`
17. PATCH `/intake/:id/status`
18. GET `/inspections`
19. POST `/inspections`
20. PATCH `/inspections/:id/resolve`
21. POST `/media/upload/start`
22. POST `/media/upload/chunk`
23. POST `/media/upload/complete`
24. GET `/evidence`
25. DELETE `/evidence/:id`
26. POST `/evidence/:id/link`
27. PATCH `/evidence/:id/legal-hold`
28. GET `/supply-entries`
29. POST `/supply-entries`
30. PATCH `/supply-entries/:id/resolve`
31. GET `/traceability`
32. POST `/traceability`
33. POST `/traceability/:id/publish`
34. POST `/traceability/:id/retract`
35. GET `/traceability/:id/steps`
36. POST `/traceability/:id/steps`
37. GET `/transfers`
38. POST `/transfers`
39. GET `/transfers/:id`
40. PATCH `/transfers/:id/status`
41. GET `/stock/movements`
42. POST `/stock/movements`
43. GET `/stock/inventory`
44. GET `/members`
45. POST `/members`
46. POST `/checkin`
47. GET `/checkin/history`
48. GET `/profile/privacy-preferences`
49. PATCH `/profile/privacy-preferences`
50. GET `/reports/summary`
51. GET `/reports/export`
52. GET `/reports/adoption-conversion`
53. GET `/audit-logs`
54. GET `/audit-logs/export`
55. GET `/users`
56. POST `/users`
57. PATCH `/users/:id`
58. DELETE `/users/:id`
59. GET `/admin/config`
60. PATCH `/admin/config`
61. GET `/admin/config/versions`
62. POST `/admin/config/rollback/:id`
63. POST `/admin/diagnostics/export`
64. GET `/admin/diagnostics/download/:id`
65. GET `/admin/jobs`
66. GET `/admin/logs`
67. POST `/admin/account-purge`
68. POST `/admin/retention-purge`
69. POST `/admin/security/rotate-key`

### API Test Mapping Table
Coverage evidence is request-level static evidence from shell tests in `repo/API_tests/*.sh` and `repo/unit_tests/*.sh`.

| Endpoint | Covered | Test Type | Test Files | Evidence |
|---|---|---|---|---|
| GET /health | yes | true no-mock HTTP | API_tests/health_api_test.sh | API_tests/health_api_test.sh:11 |
| POST /auth/register | yes | true no-mock HTTP | API_tests/auth_api_test.sh; unit_tests/auth_test.sh | API_tests/auth_api_test.sh:13 |
| POST /auth/login | yes | true no-mock HTTP | API_tests/auth_api_test.sh | API_tests/auth_api_test.sh:39 |
| GET /traceability/verify/:code | yes | true no-mock HTTP | API_tests/full_stack_test.sh | API_tests/full_stack_test.sh:168 |
| POST /auth/logout | yes | true no-mock HTTP | API_tests/auth_api_test.sh; unit_tests/auth_test.sh | API_tests/auth_api_test.sh:181 |
| GET /auth/me | yes | true no-mock HTTP | API_tests/auth_api_test.sh; API_tests/frontend_draft_test.sh | API_tests/auth_api_test.sh:59 |
| PATCH /auth/change-password | yes | true no-mock HTTP | API_tests/auth_api_test.sh | API_tests/auth_api_test.sh:157 |
| POST /account/delete | yes | true no-mock HTTP | API_tests/remediation_api_test.sh | API_tests/remediation_api_test.sh:186 |
| POST /account/cancel-deletion | yes | true no-mock HTTP | API_tests/remediation_api_test.sh | API_tests/remediation_api_test.sh:196 |
| GET /address-book | yes | true no-mock HTTP | API_tests/address_book_api_test.sh | API_tests/address_book_api_test.sh:39 |
| POST /address-book | yes | true no-mock HTTP | API_tests/address_book_api_test.sh | API_tests/address_book_api_test.sh:13 |
| PATCH /address-book/:id | yes | true no-mock HTTP | API_tests/blockers_api_test.sh | API_tests/blockers_api_test.sh:83 |
| DELETE /address-book/:id | yes | true no-mock HTTP | API_tests/address_book_api_test.sh | API_tests/address_book_api_test.sh:60 |
| GET /intake | yes | true no-mock HTTP | API_tests/intake_api_test.sh | API_tests/intake_api_test.sh:23 |
| POST /intake | yes | true no-mock HTTP | API_tests/remediation_api_test.sh; API_tests/full_stack_test.sh | API_tests/remediation_api_test.sh:51 |
| GET /intake/:id | yes | true no-mock HTTP | API_tests/intake_api_test.sh; API_tests/blockers_api_test.sh | API_tests/intake_api_test.sh:30 |
| PATCH /intake/:id/status | yes | true no-mock HTTP | API_tests/intake_api_test.sh; API_tests/remediation_api_test.sh | API_tests/intake_api_test.sh:37 |
| GET /inspections | **no** | unit-only / indirect | none | route only: src/backend/app.rs:93 |
| POST /inspections | yes | true no-mock HTTP | API_tests/intake_api_test.sh; API_tests/remediation_api_test.sh | API_tests/intake_api_test.sh:51 |
| PATCH /inspections/:id/resolve | yes | true no-mock HTTP | API_tests/intake_api_test.sh | API_tests/intake_api_test.sh:62 |
| POST /media/upload/start | yes | true no-mock HTTP | API_tests/full_stack_test.sh; API_tests/audit_fixes_test.sh | API_tests/full_stack_test.sh:78 |
| POST /media/upload/chunk | yes | true no-mock HTTP | API_tests/full_stack_test.sh; API_tests/audit_fixes_test.sh | API_tests/full_stack_test.sh:84 |
| POST /media/upload/complete | yes | true no-mock HTTP | API_tests/full_stack_test.sh; API_tests/audit_fixes_test.sh | API_tests/full_stack_test.sh:89 |
| GET /evidence | yes | true no-mock HTTP | API_tests/remediation_api_test.sh; API_tests/blockers_api_test.sh | API_tests/remediation_api_test.sh:167 |
| DELETE /evidence/:id | yes | true no-mock HTTP | API_tests/full_stack_test.sh; API_tests/remediation_api_test.sh | API_tests/full_stack_test.sh:113 |
| POST /evidence/:id/link | yes | true no-mock HTTP | API_tests/full_stack_test.sh; API_tests/acceptance_boundary_test.sh | API_tests/full_stack_test.sh:108 |
| PATCH /evidence/:id/legal-hold | yes | true no-mock HTTP | API_tests/blockers_api_test.sh; API_tests/audit_fixes_test.sh | API_tests/blockers_api_test.sh:381 |
| GET /supply-entries | yes | true no-mock HTTP | API_tests/remediation_regression_test.sh; API_tests/audit_fixes_test.sh | API_tests/remediation_regression_test.sh:178 |
| POST /supply-entries | yes | true no-mock HTTP | API_tests/full_stack_test.sh; API_tests/audit_fixes_test.sh | API_tests/full_stack_test.sh:123 |
| PATCH /supply-entries/:id/resolve | **no** | unit-only / indirect | none | route only: src/backend/app.rs:105 |
| GET /traceability | **no** | unit-only / indirect | none | route only: src/backend/app.rs:107 |
| POST /traceability | yes | true no-mock HTTP | API_tests/full_stack_test.sh; API_tests/blockers_api_test.sh | API_tests/full_stack_test.sh:160 |
| POST /traceability/:id/publish | yes | true no-mock HTTP | API_tests/full_stack_test.sh; API_tests/remediation_api_test.sh | API_tests/full_stack_test.sh:182 |
| POST /traceability/:id/retract | yes | true no-mock HTTP | API_tests/full_stack_test.sh; API_tests/blockers_api_test.sh | API_tests/full_stack_test.sh:187 |
| GET /traceability/:id/steps | yes | true no-mock HTTP | API_tests/blockers_api_test.sh; API_tests/audit_fixes_test.sh | API_tests/blockers_api_test.sh:728 |
| POST /traceability/:id/steps | yes | true no-mock HTTP | API_tests/blockers_api_test.sh | API_tests/blockers_api_test.sh:724 |
| GET /transfers | yes | true no-mock HTTP | API_tests/blockers_api_test.sh; API_tests/acceptance_boundary_test.sh | API_tests/blockers_api_test.sh:574 |
| POST /transfers | yes | true no-mock HTTP | API_tests/blockers_api_test.sh; API_tests/acceptance_boundary_test.sh | API_tests/blockers_api_test.sh:525 |
| GET /transfers/:id | **no** | unit-only / indirect | none | route only: src/backend/app.rs:113 |
| PATCH /transfers/:id/status | yes | true no-mock HTTP | API_tests/blockers_api_test.sh | API_tests/blockers_api_test.sh:545 |
| GET /stock/movements | **no** | unit-only / indirect | none | route only: src/backend/app.rs:116 |
| POST /stock/movements | yes | true no-mock HTTP | API_tests/blockers_api_test.sh; API_tests/acceptance_boundary_test.sh | API_tests/blockers_api_test.sh:589 |
| GET /stock/inventory | yes | true no-mock HTTP | API_tests/blockers_api_test.sh; API_tests/acceptance_boundary_test.sh | API_tests/blockers_api_test.sh:584 |
| GET /members | yes | true no-mock HTTP | API_tests/remediation_regression_test.sh | API_tests/remediation_regression_test.sh:211 |
| POST /members | yes | true no-mock HTTP | API_tests/full_stack_test.sh; API_tests/remediation_api_test.sh | API_tests/full_stack_test.sh:198 |
| POST /checkin | yes | true no-mock HTTP | API_tests/full_stack_test.sh; API_tests/audit_fixes_test.sh | API_tests/full_stack_test.sh:203 |
| GET /checkin/history | yes | true no-mock HTTP | API_tests/remediation_regression_test.sh | API_tests/remediation_regression_test.sh:210 |
| GET /profile/privacy-preferences | yes | true no-mock HTTP | API_tests/audit_fixes_test.sh | API_tests/audit_fixes_test.sh:329 |
| PATCH /profile/privacy-preferences | yes | true no-mock HTTP | API_tests/audit_fixes_test.sh | API_tests/audit_fixes_test.sh:340 |
| GET /reports/summary | yes | true no-mock HTTP | API_tests/full_stack_test.sh; API_tests/blockers_api_test.sh | API_tests/full_stack_test.sh:229 |
| GET /reports/export | yes | true no-mock HTTP | API_tests/full_stack_test.sh; API_tests/blockers_api_test.sh | API_tests/full_stack_test.sh:233 |
| GET /reports/adoption-conversion | yes | true no-mock HTTP | API_tests/full_stack_test.sh; API_tests/audit_fixes_test.sh | API_tests/full_stack_test.sh:241 |
| GET /audit-logs | yes | true no-mock HTTP | API_tests/full_stack_test.sh | API_tests/full_stack_test.sh:282 |
| GET /audit-logs/export | yes | true no-mock HTTP | API_tests/full_stack_test.sh; API_tests/blockers_api_test.sh | API_tests/full_stack_test.sh:290 |
| GET /users | yes | true no-mock HTTP | API_tests/auth_api_test.sh | API_tests/auth_api_test.sh:116 |
| POST /users | yes | true no-mock HTTP | API_tests/auth_api_test.sh; API_tests/address_book_api_test.sh | API_tests/auth_api_test.sh:92 |
| PATCH /users/:id | **no** | unit-only / indirect | none | route only: src/backend/app.rs:141 |
| DELETE /users/:id | yes | true no-mock HTTP | API_tests/remediation_regression_test.sh | API_tests/remediation_regression_test.sh:331 |
| GET /admin/config | yes | true no-mock HTTP | API_tests/full_stack_test.sh; API_tests/audit_fixes_test.sh | API_tests/full_stack_test.sh:251 |
| PATCH /admin/config | yes | true no-mock HTTP | API_tests/full_stack_test.sh; API_tests/blockers_api_test.sh | API_tests/full_stack_test.sh:259 |
| GET /admin/config/versions | yes | true no-mock HTTP | API_tests/full_stack_test.sh; API_tests/blockers_api_test.sh | API_tests/full_stack_test.sh:264 |
| POST /admin/config/rollback/:id | yes | true no-mock HTTP | API_tests/blockers_api_test.sh | API_tests/blockers_api_test.sh:207 |
| POST /admin/diagnostics/export | yes | true no-mock HTTP | API_tests/full_stack_test.sh; API_tests/remediation_api_test.sh | API_tests/full_stack_test.sh:268 |
| GET /admin/diagnostics/download/:id | yes | true no-mock HTTP | API_tests/remediation_api_test.sh; API_tests/acceptance_boundary_test.sh | API_tests/remediation_api_test.sh:299 |
| GET /admin/jobs | yes | true no-mock HTTP | API_tests/full_stack_test.sh; API_tests/audit_fixes_test.sh | API_tests/full_stack_test.sh:272 |
| GET /admin/logs | yes | true no-mock HTTP | API_tests/blockers_api_test.sh; API_tests/audit_fixes_test.sh | API_tests/blockers_api_test.sh:284 |
| POST /admin/account-purge | yes | true no-mock HTTP | API_tests/blockers_api_test.sh | API_tests/blockers_api_test.sh:143 |
| POST /admin/retention-purge | yes | true no-mock HTTP | API_tests/blockers_api_test.sh | API_tests/blockers_api_test.sh:386 |
| POST /admin/security/rotate-key | yes | true no-mock HTTP | API_tests/remediation_api_test.sh; API_tests/audit_fixes_test.sh | API_tests/remediation_api_test.sh:322 |

### API Test Classification
1. **True No-Mock HTTP**
   - Present broadly across shell suites using real HTTP requests (`curl`/`wget`) against a bootstrapped app at `http://localhost:8080`.
   - Evidence: `repo/run_tests.sh:105-205`, `repo/API_tests/full_stack_test.sh:23-310`, `repo/API_tests/blockers_api_test.sh:53-766`, `repo/API_tests/audit_fixes_test.sh:86-760`.
2. **HTTP with Mocking**
   - None detected in API test suites.
3. **Non-HTTP (unit/integration without HTTP)**
   - Rust unit tests embedded in backend modules: `repo/src/backend/crypto.rs:128`, `repo/src/backend/common.rs:192`, `repo/src/backend/error.rs:80`, `repo/src/backend/zip.rs:166`, `repo/src/backend/modules/evidence/handlers.rs:919`, `repo/src/backend/modules/supply/parser.rs:38`, `repo/src/backend/modules/traceability/code.rs:30`, `repo/src/backend/config.rs:131`, `repo/src/backend/modules/transfers/handlers.rs:193`.

### Mock Detection
- No explicit mock/stub framework usage found in API tests (`jest.mock`, `vi.mock`, `sinon.stub` not found in `repo/API_tests/*.sh`, `repo/unit_tests/*.sh`).
- No DI override or transport mocking evidence in API tests; request path is direct `curl/wget` through HTTP (`repo/API_tests/auth_api_test.sh:13`, `repo/API_tests/health_api_test.sh:11`).
- Rust unit tests use synthetic bytes in some helpers, but no mocking framework (`repo/src/backend/modules/evidence/handlers.rs:954-969`); classification remains non-HTTP unit testing, not mocked HTTP testing.

### Coverage Summary
- Total endpoints: **69**
- Endpoints with HTTP tests: **63**
- Endpoints with true no-mock HTTP tests: **63**
- HTTP coverage: **91.3%** (63/69)
- True API coverage: **91.3%** (63/69)
- Uncovered endpoints (6):
  - GET `/inspections`
  - PATCH `/supply-entries/:id/resolve`
  - GET `/traceability`
  - GET `/transfers/:id`
  - GET `/stock/movements`
  - PATCH `/users/:id`

### Unit Test Summary
#### Backend Unit Tests
- Unit test files/modules found:
  - `repo/src/backend/crypto.rs:128` (crypto + masking)
  - `repo/src/backend/common.rs:192` (log sanitization / date formatting)
  - `repo/src/backend/error.rs:80` (error envelope flattening)
  - `repo/src/backend/zip.rs:166` (ZIP writer)
  - `repo/src/backend/modules/supply/parser.rs:38` (parser normalization)
  - `repo/src/backend/modules/traceability/code.rs:30` (checksum)
  - `repo/src/backend/modules/evidence/handlers.rs:919` (duration/format/compression helpers)
  - `repo/src/backend/modules/transfers/handlers.rs:193` (transition helper tests)
  - `repo/src/backend/config.rs:131` (config/env parsing)
- Shell tests under `repo/unit_tests/*.sh` are **not unit tests** by strict definition; they are HTTP integration tests (`repo/unit_tests/auth_test.sh:10`, `repo/unit_tests/bootstrap_test.sh:11`).
- Important backend modules not unit-tested directly:
  - handlers without local unit test modules: `repo/src/backend/modules/auth/handlers.rs`, `repo/src/backend/modules/users/handlers.rs`, `repo/src/backend/modules/admin/handlers.rs`, `repo/src/backend/modules/checkin/handlers.rs`, `repo/src/backend/modules/dashboard/handlers.rs`, `repo/src/backend/modules/intake/handlers.rs`, `repo/src/backend/modules/inspections/handlers.rs`, `repo/src/backend/modules/profile/handlers.rs`, `repo/src/backend/modules/stock/handlers.rs`, `repo/src/backend/modules/address_book/handlers.rs`, `repo/src/backend/modules/audit/handlers.rs`.
  - middleware and jobs: `repo/src/backend/middleware/*.rs`, `repo/src/backend/jobs.rs`.

#### Frontend Unit Tests (STRICT REQUIREMENT)
- Frontend test files: **NONE** (no `*.test.*` / `*.spec.*` in frontend paths).
  - Evidence: no matches via file scan in `repo/frontend`, `repo/src/frontend`.
- Framework/tools detected for frontend unit testing: **NONE** (no file-level test evidence).
- Components/modules covered by frontend unit tests: **NONE**.
- Important frontend modules not tested:
  - app shell: `repo/src/frontend/app.rs`
  - pages: `repo/src/frontend/pages/mod.rs` and child modules (`address_book`, `admin`, `checkin`, `dashboard`, `evidence_search`, `evidence_upload`, `intake`, `login`, `profile`, `register`, `reports`, `supply`, `traceability`, `workspace`)
  - API client and shared UI logic: `repo/src/frontend/api/client.rs`, `repo/src/frontend/components/nav.rs`, `repo/src/frontend/draft.rs`.

**Frontend unit tests: MISSING**

**CRITICAL GAP:** project is inferred fullstack, and frontend unit tests are missing per strict detection rules.

#### Cross-Layer Observation
- Testing is backend/API-heavy; frontend has only shell-level bundle-string assertions and API checks (`repo/API_tests/frontend_draft_test.sh:24-191`), not frontend unit tests and not browser-level FE↔BE behavior validation.

### API Observability Check
- Strength: many tests include endpoint, request payload, and expected status/body (`repo/API_tests/auth_api_test.sh:13-218`, `repo/API_tests/audit_fixes_test.sh:86-273`).
- Weakness: substantial portions assert only status codes without response-contract depth for some endpoints (examples: `repo/API_tests/full_stack_test.sh:229-273`, `repo/API_tests/blockers_api_test.sh:545-575`).
- Verdict: **mixed / partially weak observability**.

### Tests Check
- `run_tests.sh` is Docker-based orchestration and executes suites inside container (`repo/run_tests.sh:105`, `repo/run_tests.sh:143`, `repo/run_tests.sh:161-205`) → **Docker-based: OK**.
- No package-manager runtime install commands in test orchestration.
- Local dependency note: execution still requires host Docker + curl/wget tooling (operational dependency).

### End-to-End Expectations (Fullstack)
- Expected: real FE↔BE tests.
- Present: shell-driven API/system checks and limited frontend artifact checks (`repo/API_tests/frontend_draft_test.sh`).
- Missing: browser-driven end-to-end user-flow coverage across frontend UI interactions.
- Compensation: strong API coverage partially compensates, but does **not** close frontend testing gap.

### Test Coverage Score (0-100)
**74 / 100**

### Score Rationale
- + High route-level HTTP coverage (63/69).
- + Broad security and boundary scenario testing in API suites.
- - 6 endpoints uncovered.
- - Frontend unit tests missing in fullstack project (**critical**).
- - FE↔BE E2E coverage insufficient (no browser-run test evidence).
- - Observability depth uneven across several endpoints.

### Key Gaps
1. Uncovered endpoints: GET `/inspections`, PATCH `/supply-entries/:id/resolve`, GET `/traceability`, GET `/transfers/:id`, GET `/stock/movements`, PATCH `/users/:id`.
2. Frontend unit tests absent (critical for inferred fullstack).
3. No true browser-level FE↔BE E2E tests.
4. Some endpoint tests are status-only and weak on response contract assertions.

### Confidence & Assumptions
- Confidence: **high** for static endpoint inventory and direct test-to-endpoint mapping.
- Confidence: **medium-high** for test sufficiency quality judgment (static-only constraint).
- Assumptions:
  - Endpoint inventory strictly derived from `repo/src/backend/app.rs`.
  - Coverage requires explicit method+path request evidence in tests.
  - No runtime execution performed; classification is static.

**Test Coverage Verdict: FAIL (strict mode)**

---

## 2) README Audit

README file checked: `repo/README.md`

### High Priority Issues
- Missing required explicit project type declaration at top (`backend/fullstack/web/android/ios/desktop`) (`repo/README.md:1-5`).
- Backend/fullstack startup command hard gate requires literal `docker-compose up`; README only provides `docker compose up --build` (`repo/README.md:60`).

### Medium Priority Issues
- README is very long and mixes operational runbook + feature narrative; onboarding-critical steps are not separated into strict mandatory checklist format.
- Verification guidance exists but is spread across multiple sections, making compliance audit less deterministic.

### Low Priority Issues
- Minor consistency/style issue: command variants use modern Compose syntax but hard-gate requirement is legacy literal.

### Hard Gate Failures
1. **Startup Instructions (Backend/Fullstack): FAIL**
   - Required by policy: include `docker-compose up`.
   - Found: `docker compose up --build` only (`repo/README.md:60`).

### Hard Gate Checks (Passes)
- README location exists: `repo/README.md`.
- Markdown structure/readability: present and well-structured.
- Access method (URL + port) present: `repo/README.md:63-67`.
- Verification method present (curl/API checks and UI URL): `repo/README.md:25-47`, `repo/README.md:65-67`.
- Environment policy (no runtime installs like npm/pip/apt/manual DB setup) respected in README content.
- Auth credentials and roles provided:
  - admin/staff/auditor credentials listed (`repo/README.md:19-24`).
  - role values listed (`repo/README.md:49`).

### README Verdict
**FAIL**

Rationale: at least one mandatory hard gate failed (required startup command literal).

---

## Final Verdicts
- **Test Coverage Audit:** FAIL (strict mode)
- **README Audit:** FAIL
