# Secrets Management

This reference covers **Phase 6: Secrets Management**. See the main SKILL.md for the phase summary and link.

---

17. **Design the secrets management architecture.** Secrets (API keys, database passwords, signing keys, encryption keys, TLS certificates, third-party credentials) are the most common source of security breaches when mismanaged:

    **Principle: Secrets must never exist in code, configuration files, container images, environment variables baked into images, or version control.**

    **Secrets management solution** — select and configure:
    - **HashiCorp Vault** (recommended for multi-cloud and comprehensive secrets management): Supports dynamic secrets (generate database credentials on demand with automatic expiry), PKI/certificate management, encryption as a service, and granular access policies. Operational complexity is significant — requires a dedicated team to operate.
    - **Cloud-native secrets managers** (recommended for cloud-specific deployments): AWS Secrets Manager / SSM Parameter Store, GCP Secret Manager, Azure Key Vault. Simpler to operate, integrated with cloud IAM, support automatic rotation for some secret types (RDS passwords).
    - **Kubernetes Secrets** (insufficient alone): Base64-encoded, not encrypted by default (enable encryption at rest via KMS). Acceptable for non-sensitive configuration. For actual secrets, use: External Secrets Operator (syncs from Vault/cloud secrets managers to Kubernetes Secrets), Sealed Secrets (encrypted in Git, decrypted in-cluster), or direct injection via CSI Secrets Store Driver.

    **Secrets lifecycle**:
    - **Creation**: Generate secrets with sufficient entropy (minimum 256 bits for random secrets). Use the secrets manager's generation capabilities, not application code.
    - **Distribution**: Secrets are injected into the application at runtime via: environment variables (set by the orchestrator from the secrets manager, not hardcoded in deployment manifests), mounted volumes (Vault Agent sidecar, CSI Secrets Store), or direct API calls to the secrets manager at application startup.
    - **Rotation**: Define rotation schedules per secret type:
      - Database credentials: 90 days (or use dynamic credentials with Vault — generated on demand, automatically expire).
      - API keys for third-party services: Per the third party's recommendation, or 90 days.
      - TLS certificates: 90 days (automated via cert-manager or ACME).
      - Signing keys: Annually (with overlap period for old key verification).
    - **Rotation procedure**: Must be zero-downtime. Pattern: generate new secret → update the secret in the secrets manager → application picks up the new secret (via automatic refresh or restart) → verify the new secret works → revoke the old secret after a grace period. During the grace period, both old and new secrets are valid.
    - **Revocation**: Immediate revocation capability for compromised secrets. All consuming services must handle secret revocation gracefully (detect authentication failure, alert, fall back or fail securely).
    - **Auditing**: Log every secret access (who/what accessed which secret, when, from where). Monitor for unusual access patterns (secret accessed from unexpected service, secret accessed at unusual time).

18. **Prevent secret leakage.** Secrets leak through numerous channels. Design controls for each:

    - **Version control**: Use `.gitignore` to exclude secret files. Use pre-commit hooks (e.g., `git-secrets`, `truffleHog`, `gitleaks`) to scan for secrets before commits. Scan the repository history periodically for accidentally committed secrets. If a secret is committed, rotate it immediately — removing it from Git history is insufficient because it may have been cloned, cached, or backed up.
    - **Container images**: Never bake secrets into Docker images (not in Dockerfile `ENV`, `COPY`, or build args). Scan images for secrets (Trivy, Grype). Use multi-stage builds to prevent build-time secrets from appearing in the final image.
    - **Logs**: Implement a log redaction framework that automatically detects and redacts patterns matching secrets (API keys, tokens, connection strings). Define redaction patterns (regex for common secret formats). Never log request bodies that may contain credentials. Configure structured logging to exclude sensitive fields by default.
    - **Error messages and stack traces**: Never include secrets in error messages. Ensure exception handlers do not expose environment variables or configuration details containing secrets.
    - **Client-side exposure**: Ensure server-side secrets are never sent to clients. Backend API keys, database credentials, and signing keys must never appear in JavaScript bundles, mobile app binaries, or API responses.
    - **Monitoring and alerting**: Subscribe to public secret scanning services (GitHub Secret Scanning, GitGuardian) that monitor public repositories for exposed secrets matching your patterns.