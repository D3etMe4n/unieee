# Keycloak Registration-Season Tuning & Identity Brokering Guide

## 1. Registration-Season Token Lifespan & Proactive Client Refresh Jitter

- `infra/keycloak/realm-unieee.json` sets:
  - `accessTokenLifespan: 1800` (30 minutes, within the `1800s–3600s` registration-season band)
  - `ssoSessionIdleTimeout: 3600` (60 minutes)
  - `registrationAllowed: false` (Academic Affairs provisions all accounts; self-registration is strictly disabled)
- **Recommended Frontend Refresh Jitter (Anti-Thundering Herd):**
  - Web/mobile clients using `unieee-web` (OIDC Authorization Code + PKCE `S256`) should schedule silent token refresh at a randomized fraction **`[0.70, 0.85]`** of `expires_in` (e.g., between **21m and 25.5m** for an `1800s` token), spreading `/auth/realms/unieee/protocol/openid-connect/token` traffic smoothly instead of refreshing tens of thousands of browsers at the exact expiry boundary.

---

## 2. External Identity Brokering (Microsoft Entra ID / Google Workspace — Link-Only)

To allow university students and staff to sign in via **Microsoft Entra ID** or **Google Workspace** while strictly preserving **No Self-Registration**:

1. **Pre-provision accounts via Academic Affairs (`core-java`):**
   - Create student/lecturer records via `/api/v1/admin/students` or `/api/v1/admin/lecturers` so `account.students` / `account.lecturers` and Keycloak `unieee` realm users already exist with their institutional `email` and `student_id` / `lecturer_id` attributes.
2. **Configure the OIDC Identity Provider in Keycloak (`Realm: unieee -> Identity providers`):**
   - For **Microsoft Entra ID**: use OpenID Connect v1.0 with Issuer `https://login.microsoftonline.com/<TENANT_ID>/v2.0`, Client ID/Secret from Azure App Registration, and scopes `openid profile email`.
   - For **Google Workspace**: use Google / OpenID Connect v1.0 (`https://accounts.google.com`) and restrict Hosted Domain (`hd`) to the university domain.
   - Set **Sync mode** to `IMPORT` or `FORCE` for attribute mappers only, and map `email` -> user `email`.
3. **Bind the Link-Only First Broker Login Flow (`unieee-first-broker-login-link-only`):**
   - In the Identity Provider settings, set **First login flow override** to **`unieee-first-broker-login-link-only`** (already imported in `infra/keycloak/realm-unieee.json`).
   - This custom flow executes ONLY:
     1. `idp-detect-existing-broker-user` (`REQUIRED`)
     2. `idp-auto-link` (`REQUIRED`)
   - Because `idp-create-user-if-unique` is **explicitly omitted**, any external login whose email does not match an existing Academic Affairs account in Keycloak is **rejected** rather than auto-registered.
4. **Rust & Java OIDC Compatibility:**
   - `core-java` (`spring.security.oauth2.resourceserver.jwt.jwk-set-uri`) and Rust (`OIDC_JWKS_URI`, `OIDC_ISSUER`, `OIDC_AUDIENCE`) both validate standard Keycloak-issued RS256 JWTs regardless of whether the user authenticated via local credentials or a linked Microsoft Entra ID / Google Workspace broker session.
