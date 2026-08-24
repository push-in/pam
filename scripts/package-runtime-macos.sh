#!/bin/sh

set -eu

test "$#" -eq 5 || {
    echo "Usage: package-runtime-macos.sh <pam-binary> <php-sdk> <version> <target> <output-directory>" >&2
    exit 64
}

pam_binary=$1
php_sdk=$2
pam_version=$3
pam_target=$4
output_directory=$5
package="pam-${pam_version}-${pam_target}"
package_root="${output_directory}/${package}"

test -x "${pam_binary}" || {
    echo "PAM binary is not executable: ${pam_binary}" >&2
    exit 66
}
test -f "${php_sdk}/lib/libphp.dylib" || {
    echo "The verified macOS PHP Embed library is missing from ${php_sdk}." >&2
    exit 69
}
test -f runtime/catalog.json || {
    echo "PAM runtime catalog is missing from the release checkout." >&2
    exit 66
}
test -f pam-native/Cargo.toml || {
    echo "PAM Native SDK source is missing from the release checkout." >&2
    exit 66
}

mkdir -p \
    "${package_root}/bin" \
    "${package_root}/etc/conf.d" \
    "${package_root}/lib" \
    "${package_root}/share/pam/native" \
    "${package_root}/share/pam/runtime"
cp "${pam_binary}" "${package_root}/bin/pam"
cp "${php_sdk}/lib/libphp.dylib" "${package_root}/lib/libphp.dylib"
cp runtime/catalog.json "${package_root}/share/pam/runtime/catalog.json"

cat > "${package_root}/etc/php.ini" <<'EOF'
expose_php=Off
display_errors=Off
log_errors=On
variables_order=GPCS
zend.assertions=-1
EOF

if test -d runtime/ios; then
    cp -R runtime/ios "${package_root}/share/pam/runtime/ios"
fi

tar \
    --exclude='./target' \
    --exclude='./examples' \
    --exclude='./runtime-builder' \
    --exclude='*/build' \
    --exclude='*/DerivedData' \
    --exclude='*/.build' \
    --exclude='*/.swiftpm' \
    --exclude='*/.gradle' \
    --exclude='*/.kotlin' \
    --exclude='*/.cxx' \
    --exclude='*/local.properties' \
    -C pam-native -cf - . |
    tar -C "${package_root}/share/pam/native" -xf -

cat > "${package_root}/bin/pam-run" <<'EOF'
#!/bin/sh
set -eu

launcher=$0
while test -L "${launcher}"; do
    launcher_directory=$(CDPATH= cd -- "$(dirname -- "${launcher}")" && pwd)
    launcher_target=$(readlink "${launcher}")
    case "${launcher_target}" in
        /*) launcher=${launcher_target} ;;
        *) launcher=${launcher_directory}/${launcher_target} ;;
    esac
done
launcher_directory=$(CDPATH= cd -- "$(dirname -- "${launcher}")" && pwd)
pam_install_root=$(CDPATH= cd -- "${launcher_directory}/.." && pwd)
export PAM_HOME="${pam_install_root}/share/pam"
export DYLD_LIBRARY_PATH="${pam_install_root}/lib${DYLD_LIBRARY_PATH:+:${DYLD_LIBRARY_PATH}}"
export PHPRC="${pam_install_root}/etc/php.ini"
export PHP_INI_SCAN_DIR="${pam_install_root}/etc/conf.d${PAM_PHP_INI_SCAN_DIR:+:${PAM_PHP_INI_SCAN_DIR}}"
export PATH="${pam_install_root}/bin:${PATH}"
exec "${pam_install_root}/bin/pam" "$@"
EOF
chmod 0755 "${package_root}/bin/pam" "${package_root}/bin/pam-run"
ln -s pam-run "${package_root}/bin/php"

cp LICENSE LICENSING.md README.md "${package_root}/"
tar -C "${output_directory}" -czf "${output_directory}/${package}.tar.gz" "${package}"
(
    cd "${output_directory}"
    shasum -a 256 "${package}.tar.gz" > "${package}.tar.gz.sha256"
)
