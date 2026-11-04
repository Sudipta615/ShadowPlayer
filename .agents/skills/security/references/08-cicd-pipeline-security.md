# CI/CD Pipeline Security

This reference covers **Phase 9: CI/CD Pipeline Security**. See the main SKILL.md for the phase summary and link.

---

25. **Design secure CI/CD pipelines.** The CI/CD pipeline has access to source code, secrets, and production infrastructure — it is a high-value target:

    **Build integrity**:
    - **Source integrity**: All code changes go through version control with required code review. Branch protection: require pull request approval, status checks passing, no force-push to main/production branches. Sign commits (GPG) for high-security environments.
    - **Build reproducibility**: Builds should be deterministic — the same source code produces the same artifact. Use lock files for dependencies (package-lock.json, Pipfile.lock, go.sum). Pin all tool versions in the build configuration.
    - **Build environment isolation**: Build jobs run in ephemeral, isolated environments (clean containers) that are destroyed after each build. No persistent state between builds. No shared build agents for different security zones (don't build production and open-source projects on the same agent).
    - **Artifact signing**: Sign build artifacts (container images, binaries) with a cryptographic signature. Verify signatures before deployment. Use Sigstore/cosign for container image signing. This ensures the artifact deployed is the artifact that was built and scanned.

    **Pipeline secrets**:
    - CI/CD secrets (deployment credentials, registry credentials, signing keys) must be stored in the CI/CD platform's secrets management (GitHub Actions secrets, GitLab CI variables, CircleCI contexts) or in an external secrets manager. Never in the pipeline configuration file.
    - Scope secrets to the minimum: if a secret is only needed in the deploy stage, it should not be available in the build or test stages.
    - Rotate CI/CD secrets on the same schedule as other secrets.
    - Audit who has access to CI/CD secrets and pipeline configuration.

    **Pipeline stages for security**:
    - **SAST (Static Application Security Testing)**: Scan source code for security vulnerabilities (SQL injection patterns, hardcoded secrets, insecure function usage). Run on every commit. Tools: Semgrep (recommended for customizable rules), SonarQube, CodeQL, Bandit (Python), Gosec (Go).
    - **SCA (Software Composition Analysis)**: Scan dependencies for known vulnerabilities. Run on every commit and on a daily schedule (new CVEs are published daily). Tools: Snyk, Dependabot, Renovate, OWASP Dependency-Check.
    - **Container image scanning**: Scan the built image before pushing to the registry. Block images with Critical vulnerabilities.
    - **Secret scanning**: Scan the codebase and CI/CD configuration for leaked secrets. Tools: gitleaks, truffleHog, GitHub secret scanning.
    - **IaC security scanning**: Scan Terraform, CloudFormation, Kubernetes manifests for security misconfigurations. Tools: tfsec, checkov, KICS.
    - **License compliance scanning**: Identify dependencies with incompatible licenses (if applicable).

    **Deployment security**:
    - Deployments to production should only be triggered from the main branch after all quality and security checks pass. No direct deployments from developer machines.
    - Require approval gates for production deployments (at least for critical services).
    - Deployment credentials should be scoped to the minimum required (deploy to specific cluster/service, not admin access to the entire cloud account).
    - Log all deployments with: who triggered, what version, when, where, and the result.