# pam/psr-bridge

PSR-7, PSR-15 and PSR-17 interoperability for Pam using the official PHP-FIG
interfaces.

```bash
pam composer require pam/psr-bridge
```

Pass a PSR-15 handler and middleware to `Pam\App::handler()` and
`Pam\App::middleware()`; Pam converts native requests and responses at the runtime
boundary.

## License

Free and open-source under the [Apache License 2.0](LICENSE). You may use,
modify, and distribute this package for any purpose, including commercially.
