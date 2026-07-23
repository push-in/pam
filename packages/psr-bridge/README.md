# pushinbr/pam-psr-bridge

PSR-7, PSR-15 and PSR-17 interoperability for Pam using the official PHP-FIG
interfaces.

```bash
pam composer require pushinbr/pam-psr-bridge
```

Pass a PSR-15 handler and middleware to `Pam\App::handler()` and
`Pam\App::middleware()`; Pam converts native requests and responses at the runtime
boundary.
