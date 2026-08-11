# Governance

PAM is maintainer-led. Maintainers are responsible for release integrity,
security response, compatibility policy and final technical decisions.

Routine fixes and documentation changes proceed through pull-request review.
Changes to stable public contracts, security boundaries, persistence semantics,
wire formats or release policy require an issue or discussion before
implementation, executable compatibility coverage and an explicit migration
story when behavior changes.

Decisions favor, in order:

1. user and supply-chain safety;
2. correctness and bounded resource use;
3. compatibility with PHP, Composer and Laravel contracts;
4. operability and debuggability;
5. measured performance;
6. implementation convenience.

Contributor access is earned through sustained, constructive participation and
sound reviews. Maintainers may grant or remove repository roles based on the
needs and safety of the project. Security reports and conduct cases remain
confidential to the smallest practical maintainer group.

If consensus is not reached, the repository owner makes the final decision and
documents material trade-offs publicly unless security or privacy prevents it.
