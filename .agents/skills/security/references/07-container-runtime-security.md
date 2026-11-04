# Container and Runtime Security

This reference covers **Phase 8: Container and Runtime Security**. See the main SKILL.md for the phase summary and link.

---

22. **Design container image security.** Container images are the application's packaging — they inherit every vulnerability in their base layers:

    **Base image selection**:
    - Use minimal base images: `distroless` (Google), Alpine, or language-specific slim images. Smaller images have fewer packages, fewer vulnerabilities, and smaller attack surface.
    - Never use `latest` tag — pin to a specific version for reproducibility. Example: `python:3.12.1-slim-bookworm`, not `python:latest`.
    - Prefer official images from trusted publishers. Verify image provenance.

    **Image hardening**:
    - **Run as non-root**: The container process must run as a non-root user. In the Dockerfile: `RUN adduser --disabled-password appuser` and `USER appuser`. In Kubernetes: `securityContext.runAsNonRoot: true`.
    - **Read-only filesystem**: Mount the container's root filesystem as read-only (`readOnlyRootFilesystem: true`). Mount writable volumes only for directories that need writes (tmp, cache).
    - **No unnecessary packages**: Do not install debug tools, shells, package managers, or compilers in production images. Use multi-stage builds: compile in a build stage, copy only the binary/artifacts to the runtime stage.
    - **No secrets in images**: No credentials, keys, or configuration secrets in the image layers (see the Secrets Management reference).
    - **Set resource limits**: Define CPU and memory limits in the Kubernetes pod spec. Prevents denial-of-service from runaway processes and limits the impact of cryptomining malware.

    **Image scanning**:
    - Scan images for known vulnerabilities (CVEs) in CI/CD before deployment: Trivy, Grype, Snyk Container, AWS ECR scanning.
    - Define a vulnerability policy: Block deployment of images with Critical or High vulnerabilities. Allow Medium/Low with review and a remediation timeline.
    - Scan images in the registry continuously — new CVEs are published daily, and a previously clean image may become vulnerable.
    - Scan for secrets in image layers (Trivy can detect embedded secrets).

23. **Design Kubernetes security** (if applicable):

    **Pod security**:
    - Enforce Pod Security Standards (PSS): Use `Restricted` profile for all workloads. This enforces: non-root, read-only filesystem, no privilege escalation, no host networking, restricted volume types, and seccomp profiles.
    - Drop all Linux capabilities: `securityContext.capabilities.drop: ["ALL"]`. Add back only the specific capabilities needed (rare for most applications).
    - Disable service account token auto-mounting for pods that do not need Kubernetes API access: `automountServiceAccountToken: false`.

    **Network policies**:
    - Default deny all ingress and egress traffic for each namespace.
    - Explicitly allow only the traffic flows that are necessary: "order-service can receive traffic from the API gateway on port 8080. order-service can connect to the orders-db on port 5432. All other traffic is blocked."
    - Network policies enforce microsegmentation at the pod level, limiting lateral movement if a pod is compromised.

    **RBAC (Kubernetes RBAC)**:
    - Restrict who can perform what actions in the cluster. Developers should have limited production access (view logs, describe pods). Only CI/CD service accounts should deploy.
    - Never grant `cluster-admin` to application service accounts.
    - Audit RBAC configurations periodically. Use tools like `rbac-lookup`, `kubectl-who-can`.

    **Secrets in Kubernetes**:
    - Enable encryption at rest for Kubernetes Secrets (KMS provider).
    - Prefer external secrets management (External Secrets Operator) over native Kubernetes Secrets for sensitive credentials.
    - Restrict Secret access per namespace. Pods in namespace A should not access Secrets in namespace B.

    **Admission controllers**:
    - Use admission controllers (OPA/Gatekeeper, Kyverno) to enforce security policies at deployment time: block privileged containers, enforce image registry allow-lists (only allow images from your trusted registry), require resource limits, enforce labels and annotations for ownership tracking.

24. **Design runtime security.** Detect and respond to threats at runtime:
    - **Runtime anomaly detection**: Use tools (Falco, Sysdig, Aqua Runtime Protection) that monitor container behavior and alert on anomalies: unexpected process execution (shell spawned in an application container), unexpected network connections, file system changes in read-only volumes, privilege escalation attempts.
    - **Immutable infrastructure**: Once a container is deployed, it should not be modified. No SSH into containers, no package installations, no hot-patching. If a fix is needed, build a new image and redeploy. This ensures the running software matches the audited, scanned, and tested image.
    - **Seccomp and AppArmor profiles**: Apply seccomp profiles to restrict the system calls containers can make. Use the `RuntimeDefault` seccomp profile at minimum. Custom profiles provide tighter restrictions for known workloads.