# Infrastructure Security

This reference covers **Phase 7: Infrastructure Security**. See the main SKILL.md for the phase summary and link.

---

19. **Design cloud IAM security.** Identity and Access Management in cloud environments is the primary control plane — misconfigured IAM is the #1 cause of cloud breaches:

    **Principle of least privilege**:
    - Every IAM role, policy, and service account must have the minimum permissions necessary for its function. Start with zero permissions and add only what is needed.
    - **Never use wildcard permissions** (`*`) in production policies. `"Action": "s3:*"` should be `"Action": ["s3:GetObject", "s3:PutObject"]` on specific resources.
    - **Never use `*` resources** unless the action genuinely applies to all resources. Scope to specific ARNs/resource identifiers.
    - **Avoid long-lived credentials**: Use IAM roles (AWS), workload identity (GCP), managed identity (Azure) instead of access keys. Workload identity is automatically rotated and cannot be leaked in code.
    - **If access keys are necessary**: Rotate every 90 days. Disable unused keys. Never share keys across services. Monitor for usage of keys from unexpected IP ranges.

    **IAM policy review**:
    - Use cloud-native tools to identify overly permissive policies: AWS IAM Access Analyzer, GCP IAM Recommender, Azure Advisor.
    - Review all IAM policies quarterly. Look for: policies with `*` actions or resources, policies attached directly to users (use groups or roles instead), unused roles and policies, cross-account access grants that are no longer needed.
    - Enable CloudTrail (AWS), Cloud Audit Logs (GCP), Azure Activity Log for all IAM actions. Alert on: root account usage, policy changes, new role creation, cross-account role assumption.

    **Service account management**:
    - Each service/workload has its own service account with scoped permissions.
    - Never share service accounts across services.
    - Never use the default service account (it often has overly broad permissions).
    - Audit service account key usage. Prefer keyless authentication (workload identity federation, IRSA, GKE workload identity).

20. **Design network security.** Network segmentation limits the blast radius of a breach:

    **VPC and subnet design**:
    - **Public subnets**: Only for load balancers and bastion hosts (if used). No application servers or databases in public subnets.
    - **Private subnets**: Application servers, worker nodes, internal services. No direct internet access. Outbound internet access via NAT Gateway (for dependency downloads, external API calls).
    - **Isolated/data subnets**: Databases, caches, secrets stores. Accessible only from application subnets via security groups.

    **Security groups / firewall rules**:
    - Default deny: All traffic is denied unless explicitly allowed.
    - Define rules per service: "order-service can reach the orders database on port 5432. Nothing else can."
    - No `0.0.0.0/0` ingress rules on anything except the public load balancer on ports 80/443.
    - Review security group rules quarterly. Remove rules that are no longer needed.

    **Network access controls**:
    - **Database**: Not publicly accessible. Accessible only from application subnets. Use security groups and VPC peering/PrivateLink, not public endpoints with IP allow-lists.
    - **Admin interfaces**: Access via VPN, bastion host, or zero-trust access proxy (e.g., Cloudflare Access, Tailscale, BeyondCorp). Never expose admin interfaces to the public internet.
    - **Service mesh**: For microservices, consider a service mesh (Istio, Linkerd) that provides mTLS between services, fine-grained traffic policies, and observability without application code changes.

    **DDoS protection**:
    - Use cloud-native DDoS protection (AWS Shield, GCP Cloud Armor, Azure DDoS Protection, Cloudflare) on all internet-facing endpoints.
    - Configure WAF (Web Application Firewall) rules for common attack patterns (SQL injection, XSS, bot detection). Start with managed rule sets, then customize.
    - Rate limiting at the edge (CDN/WAF) for all endpoints, with stricter limits on authentication endpoints (see the authentication skill).
    - Design the application to be resilient under load: connection limits, request queuing, graceful degradation.

21. **Design egress security.** Control what your services can access on the internet:
    - **Egress filtering**: Restrict outbound connections from application subnets to only the external services the application needs. Use a network firewall, NAT Gateway with security groups, or an egress proxy.
    - **Allow-list outbound destinations**: Maintain a list of approved external domains/IPs that services can reach (payment gateways, email providers, third-party APIs). Block everything else.
    - **DNS filtering**: Use DNS-based filtering to block connections to known malicious domains.
    - **Egress monitoring**: Log all outbound connections. Alert on: connections to unexpected destinations, large data transfers outbound (potential data exfiltration), and connections to known malicious IPs/domains.
    - **Why this matters**: If an attacker compromises an application server, egress controls prevent them from exfiltrating data, downloading additional tools, or establishing command-and-control channels.