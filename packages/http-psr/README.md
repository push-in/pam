# PAM HTTP PSR

PSR-7, PSR-15 and PSR-17 interoperability for Pam using the official PHP-FIG
interfaces.

## Start here

PAM PSR is a Composer package for the PAM Runtime; it is not a standalone
runtime. Install PAM first, open your application directory, and add the
package through PAM's Composer toolchain:

```bash
curl --proto '=https' --proto-redir '=https' --tlsv1.2 \
    --connect-timeout 15 --max-time 60 --max-filesize 1048576 -fsSL \
    https://github.com/push-in/pam/releases/latest/download/install.sh | sh

pam doctor
cd my-app
pam composer require pushinbr/pam-http-psr
```

Pass a PSR-15 handler and middleware to `Pam\App::handler()` and
`Pam\App::middleware()`; Pam converts native requests and responses at the runtime
boundary.

## License

Free and open-source under the [Apache License 2.0](LICENSE). You may use,
modify, and distribute this package for any purpose, including commercially.
