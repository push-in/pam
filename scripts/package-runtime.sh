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
    "${package_root}/share/pam/runtime"

cp "${pam_binary}" "${package_root}/bin/pam"
cp "${php_library}" "${package_root}/lib/libphp.so"

test -f pam-native/Cargo.toml || {
    echo "Pam Native SDK source is missing from the release checkout." >&2
    exit 66
}
tar \
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
cp runtime/catalog.json "${package_root}/share/pam/runtime/catalog.json"

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
modules='timezonedb opcache bcmath calendar ctype curl dom exif ffi fileinfo ftp gd gettext iconv intl mbstring mysqlnd mysqli pdo pdo_mysql pdo_pgsql pdo_sqlite pgsql phar posix shmop simplexml soap sockets sqlite3 sysvmsg sysvsem sysvshm tokenizer xml xmlreader xmlwriter xsl zip'
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
PAM_INSTALL_ROOT=$(CDPATH= cd -- "$(dirname -- "$PAM_LAUNCHER")/.." && pwd)
export PAM_HOME="$PAM_INSTALL_ROOT/share/pam"
export LD_LIBRARY_PATH="$PAM_INSTALL_ROOT/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export PAM_EXTENSION_DIR="$PAM_INSTALL_ROOT/lib/php/extensions"
export PHPRC="$PAM_INSTALL_ROOT/etc/php.ini"
export PHP_INI_SCAN_DIR="$PAM_INSTALL_ROOT/etc/conf.d${PAM_PHP_INI_SCAN_DIR:+:$PAM_PHP_INI_SCAN_DIR}"
export PATH="$PAM_INSTALL_ROOT/bin:$PATH"
exec "$PAM_INSTALL_ROOT/bin/pam" "$@"
EOF
chmod 0755 "${package_root}/bin/pam" "${package_root}/bin/pam-run"
ln -s pam-run "${package_root}/bin/php"

cp LICENSE LICENSING.md README.md "${package_root}/"
tar -C "${output_directory}" -czf "${output_directory}/${package}.tar.gz" "${package}"
(
    cd "${output_directory}"
    sha256sum "${package}.tar.gz" > "${package}.tar.gz.sha256"
)
