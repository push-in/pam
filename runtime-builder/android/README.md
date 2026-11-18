# PAM PHP Runtime for Android

This builder produces the static PHP Embed runtime shipped with Pam Native.
It downloads the official PHP source archive, validates its pinned SHA-256,
applies the smallest Android portability patch, and cross-compiles with the
pinned Android NDK.

```bash
export ANDROID_HOME="$HOME/Android/Sdk"
./runtime-builder/android/build.sh --php 8.5 all
```

Available releases and their official source checksums are pinned in
`runtime/catalog.json`. PHP 8.4 and 8.5 can coexist:

```bash
./runtime-builder/android/build.sh --php 8.4 all
./runtime-builder/android/build.sh --php 8.5 all
```

`all` builds `arm64-v8a`, `armeabi-v7a`, `x86` and `x86_64`; one ABI can be
named instead (`./runtime-builder/android/build.sh --php 8.5 armeabi-v7a`).

On the 32-bit ABIs (`armeabi-v7a`, `x86`) PHP keeps 64-bit integers
(`PHP_INT_SIZE` 8, as on the x32 ABI), so PHP code and the PAM Native bridge
see the same integers on every device. `patches/0002-android-32-bit-php-integers.patch`
enables 64-bit `zend_long` on 32-bit Android, keeps it 8-byte aligned on x86
(Zend stores flags in the low bits of `Bucket` offsets), and replaces the
32-bit i386/ARM assembly paths that assumed a 32-bit `zend_long`. The opcache
JIT is disabled there; the bytecode cache stays on.

The resulting `runtime/android/<php-version>-r<revision>/<abi>` directories are
release inputs and stay out of Git. Release automation must archive them,
publish a SHA-256 manifest, and sign that manifest. No unverified third-party
PHP binary is accepted by the Pam build.

The MVP runtime intentionally keeps networking and storage in Android native
modules. Its PHP extension set is: Core, date, PCRE, ctype, filter, hash, JSON,
Phar, random, Reflection, session, SPL, standard, and tokenizer.
