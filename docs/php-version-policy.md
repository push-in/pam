# PHP version policy

PAM embeds **PHP 8.5 by default**. This is a product invariant, not merely the newest entry in a
test matrix.

- Official PAM runtime artifacts embed PHP 8.5.
- `pam init` projects and the official skeleton require PHP `^8.5`.
- Android and Apple builders select the `8.5` catalog channel unless an operator explicitly sets
  `PAM_PHP_VERSION` or passes `--php`.
- Release, container, security, and primary compatibility workflows build against PHP 8.5.

PHP 8.4 may remain in explicit compatibility matrices while maintained. Supporting an older PHP
series does not make it the default and must not change generated manifests, bundled artifacts, or
implicit runtime selection.

The default may move again only as an intentional, documented runtime release with regenerated
artifacts and clean-host certification. Patch releases within the 8.5 channel may advance without
changing this policy.
