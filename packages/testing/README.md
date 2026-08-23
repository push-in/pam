# PAM Testing compatibility package

## Start here

This package preserves existing `pushinbr/pam-testing` installations. New applications must use:

```bash
pam composer require --dev pushinbr/pam-http-testing
```

The replacement keeps aliases for the legacy `Pam\Testing` namespace while new code uses
`Pam\Http\Testing`.

Install PAM before using either package. This bridge exists only for migration and is abandoned in
Composer in favor of `pushinbr/pam-http-testing`.
