# Dependency and Supply Chain Security

This reference covers **Phase 10: Dependency and Supply Chain Security**. See the main SKILL.md for the phase summary and link.

---

26. **Design dependency security management.** Third-party dependencies are a major attack surface — you inherit every vulnerability in every dependency:

    **Dependency inventory**:
    - Maintain a Software Bill of Materials (SBOM) for every service. An SBOM lists every direct and transitive dependency with its version. Generate SBOMs automatically during the build process (Syft, CycloneDX, SPDX).
    - The SBOM enables rapid response when a new vulnerability is announced — you can immediately identify which services are affected.

    **Vulnerability scanning and remediation**:
    - **Automated scanning**: Run SCA on every commit and on a daily schedule. Configure the tool to scan both direct and transitive dependencies.
    - **Vulnerability prioritization**: Not all CVEs are equally urgent. Prioritize based on:
      - Severity (CVSS score): Critical (9.0+), High (7.0-8.9), Medium (4.0-6.9), Low (< 4.0).
      - Exploitability: Is there a known exploit in the wild? (Check CISA KEV, Exploit-DB.)
      - Reachability: Does the application actually use the vulnerable function/component? (Some tools can analyze reachability — Snyk, Semgrep Supply Chain.)
      - Exposure: Is the vulnerable component in an internet-facing service or a background batch job?
    - **Remediation SLAs**: Define maximum time to remediate based on priority:
      - Critical with known exploit: 24-72 hours.
      - Critical without known exploit: 7 days.
      - High: 30 days.
      - Medium: 90 days.
      - Low: Best effort, next regular update cycle.
    - **Automated dependency updates**: Use Dependabot, Renovate, or similar tools to automatically create pull requests for dependency updates. Configure to auto-merge patch updates with passing tests.

    **Dependency selection criteria**:
    - Before adding a new dependency, evaluate: Is it actively maintained (recent commits, responsive to issues)? Does it have a history of security vulnerabilities? What is its dependency footprint (does it pull in dozens of transitive dependencies)? Is it from a trusted source/organization? Is there a simpler alternative with fewer dependencies?
    - Prefer dependencies with fewer transitive dependencies — each transitive dependency increases the attack surface.
    - Pin dependency versions explicitly. Use lock files. Verify integrity checksums.

    **Supply chain attack mitigation**:
    - **Registry security**: Use a private registry (Artifactory, Nexus, cloud-native registries) that proxies public registries. This provides caching, scanning, and the ability to block compromised packages.
    - **Typosquatting defense**: Review dependency names carefully. Use tools that detect typosquatting (similar package names that are malicious).
    - **Lock file integrity**: Commit lock files to version control. Review lock file changes in pull requests — unexpected changes may indicate dependency tampering.
    - **Dependency review in PRs**: Require review of new dependency additions. Adding a new dependency is a security decision and should be treated as such.