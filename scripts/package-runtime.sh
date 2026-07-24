#!/bin/sh

set -eu

test "$#" -eq 4 || {
    echo "Usage: package-runtime.sh <pam-binary> <version> <target> <output-directory>" >&2
    exit 64
}

pam_binary=$1
pam_version=$2
pam_target=$3
output_directory=$4
package="pam-${pam_version}-${pam_target}"
package_root="${output_directory}/${package}"

test -x "${pam_binary}" || {
    echo "PAM binary is not executable: ${pam_binary}" >&2
    exit 66
}

php_library=$(ldd "${pam_binary}" | awk '/libphp/{print $3; exit}')
test -f "${php_library}" || {
    echo "The PHP Embed library linked by PAM could not be found." >&2
    exit 69
}

php_api=$(php-config --phpapi)
extension_directory="/usr/lib/php/${php_api}"
if test ! -d "${extension_directory}"; then
    extension_directory=$(php-config --extension-dir)
fi
test -d "${extension_directory}" || {
    echo "PHP extension directory could not be found." >&2
    exit 69
}

mkdir -p \
    "${package_root}/bin" \
    "${package_root}/etc/conf.d" \
    "${package_root}/lib/php/extensions" \
    "${package_root}/share/pam/native" \
    "${package_root}/share/pam/mobile-ui"

cp "${pam_binary}" "${package_root}/bin/pam"
cp "${php_library}" "${package_root}/lib/libphp.so"

test -f pam-native/Cargo.toml || {
    echo "Pam Native SDK source is missing from the release checkout." >&2
    exit 66
}
test -f pam-mobile-ui/composer.json || {
    echo "PAM Mobile UI source is missing from the release checkout." >&2
    exit 66
}
for android_abi in arm64-v8a x86_64; do
    android_runtime="pam-native/runtime/android/${android_abi}"
    test -s "${android_runtime}/lib/libphp.a" || {
        echo "PHP Android runtime library is missing for ${android_abi}." >&2
        exit 66
    }
    test -f "${android_runtime}/include/php/main/php.h" || {
        echo "PHP Android headers are missing for ${android_abi}." >&2
        exit 66
    }
    test -f "${android_runtime}/include/php/sapi/embed/php_embed.h" || {
        echo "PHP Android Embed headers are missing for ${android_abi}." >&2
        exit 66
    }
    test -f "${android_runtime}/runtime.json" || {
        echo "PHP Android runtime provenance is missing for ${android_abi}." >&2
        exit 66
    }
    case "${android_abi}" in
        arm64-v8a) rust_target=aarch64-linux-android ;;
        x86_64) rust_target=x86_64-linux-android ;;
    esac
    engine_library="pam-native/target/${rust_target}/release/libpam_native_engine.a"
    test -s "${engine_library}" || {
        echo "Prebuilt Pam Native engine is missing for ${android_abi}." >&2
        exit 66
    }
done
tar \
    --exclude='./.git' \
    --exclude='./target' \
    --exclude='./examples' \
    --exclude='./runtime-builder' \
    --exclude='*/build' \
    --exclude='*/.gradle' \
    --exclude='*/.kotlin' \
    --exclude='*/.cxx' \
    --exclude='*/local.properties' \
    -C pam-native -cf - . |
    tar -C "${package_root}/share/pam/native" -xf -
for rust_target in aarch64-linux-android x86_64-linux-android; do
    engine_directory="${package_root}/share/pam/native/target/${rust_target}/release"
    mkdir -p "${engine_directory}"
    cp \
        "pam-native/target/${rust_target}/release/libpam_native_engine.a" \
        "${engine_directory}/"
done
tar \
    --exclude='./.git' \
    --exclude='./.build' \
    --exclude='./examples' \
    --exclude='./tools/phpstan/vendor' \
    --exclude='*/build' \
    --exclude='*/.gradle' \
    -C pam-mobile-ui -cf - . |
    tar -C "${package_root}/share/pam/mobile-ui" -xf -

copy_dependencies() {
    ldd "$1" | awk '$2 == "=>" { print $1 "|" $3 }' |
        while IFS='|' read -r library_name library_path; do
            case "${library_name}" in
                libc.so.*|libm.so.*|libpthread.so.*|libdl.so.*|librt.so.*|\
                libresolv.so.*|libutil.so.*|libanl.so.*|libnss_*.so.*)
                    continue
                    ;;
            esac

            test "${library_path}" != "not" && test -f "${library_path}" || {
                echo "Runtime dependency not found for $1: ${library_name}" >&2
                exit 69
            }

            bundled_library="${package_root}/lib/${library_name}"
            test -f "${bundled_library}" && continue
            cp -L "${library_path}" "${bundled_library}"
            copy_dependencies "${bundled_library}"
        done
}

cat > "${package_root}/etc/php.ini" <<'EOF'
expose_php=Off
display_errors=Off
log_errors=On
variables_order=GPCS
zend.assertions=-1
EOF

# This is the reviewed extension surface shipped by PAM. It does not inherit
# arbitrary extensions enabled on the build machine.
modules='opcache bcmath calendar ctype curl dom exif ffi fileinfo ftp gd gettext iconv intl mbstring mysqlnd mysqli pdo pdo_mysql pdo_pgsql pdo_sqlite pgsql phar posix shmop simplexml soap sockets sqlite3 sysvmsg sysvsem sysvshm tokenizer xml xmlreader xmlwriter xsl zip'
priority=10
for module in ${modules}; do
    module_file="${extension_directory}/${module}.so"
    test -f "${module_file}" || continue
    cp "${module_file}" "${package_root}/lib/php/extensions/${module}.so"
    if test "${module}" = opcache; then
        directive="zend_extension=\${PAM_EXTENSION_DIR}/${module}.so"
    else
        directive="extension=\${PAM_EXTENSION_DIR}/${module}.so"
    fi
    copy_dependencies "${package_root}/lib/php/extensions/${module}.so"
    printf '%s\n' "${directive}" > \
        "${package_root}/etc/conf.d/$(printf '%02d' "${priority}")-${module}.ini"
    priority=$((priority + 1))
done

copy_dependencies "${package_root}/bin/pam"
copy_dependencies "${package_root}/lib/libphp.so"

cat > "${package_root}/bin/pam-run" <<'EOF'
#!/bin/sh
set -eu
PAM_LAUNCHER=$(readlink -f -- "$0")
PAM_HOME=$(CDPATH= cd -- "$(dirname -- "$PAM_LAUNCHER")/.." && pwd)
export LD_LIBRARY_PATH="$PAM_HOME/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export PAM_EXTENSION_DIR="$PAM_HOME/lib/php/extensions"
export PHPRC="$PAM_HOME/etc/php.ini"
export PHP_INI_SCAN_DIR="$PAM_HOME/etc/conf.d${PAM_PHP_INI_SCAN_DIR:+:$PAM_PHP_INI_SCAN_DIR}"
exec "$PAM_HOME/bin/pam" "$@"
EOF
chmod 0755 "${package_root}/bin/pam" "${package_root}/bin/pam-run"

cp LICENSE README.md "${package_root}/"
tar -C "${output_directory}" -czf "${output_directory}/${package}.tar.gz" "${package}"
(
    cd "${output_directory}"
    sha256sum "${package}.tar.gz" > "${package}.tar.gz.sha256"
)
