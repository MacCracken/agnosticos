#!/bin/bash
#
# SBOM (Software Bill of Materials) Generator for AGNOS
# Generates SBOM in SPDX and CycloneDX formats
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
OUTPUT_DIR="${PROJECT_ROOT}/sbom"
VERSION=$(grep -E '^version' "${PROJECT_ROOT}/userland/Cargo.toml" | head -1 | sed 's/.*= *"\(.*\)".*/\1/')

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Create output directory
mkdir -p "${OUTPUT_DIR}"

# Generate Rust SBOM using cargo-bom
generate_rust_sbom() {
    log_info "Generating Rust dependencies SBOM..."
    
    cd "${PROJECT_ROOT}/userland"
    
    # Check if cargo-bom is installed
    if ! command -v cargo-bom &> /dev/null; then
        log_warn "cargo-bom not found, installing..."
        cargo install cargo-bom
    fi
    
    # Generate BOM in JSON format
    cargo bom > "${OUTPUT_DIR}/rust-dependencies.txt"
    
    # Generate detailed JSON with cargo-license
    if command -v cargo-license &> /dev/null; then
        cargo license --json > "${OUTPUT_DIR}/rust-licenses.json"
    else
        log_warn "cargo-license not found, skipping license details"
    fi
    
    log_info "Rust SBOM generated: ${OUTPUT_DIR}/rust-dependencies.txt"
}

# Generate Python SBOM
generate_python_sbom() {
    log_info "Generating Python dependencies SBOM..."
    
    cd "${PROJECT_ROOT}"
    
    if [ -f "requirements.txt" ]; then
        # Create requirements with hashes
        pip-compile --generate-hashes requirements.txt -o "${OUTPUT_DIR}/python-requirements.txt" 2>/dev/null || \
            cp requirements.txt "${OUTPUT_DIR}/python-requirements.txt"
    fi
    
    # Find all Python dependencies in scripts
    find scripts -name "*.py" -exec grep -h "^import\|^from" {} \; 2>/dev/null | \
        sort | uniq > "${OUTPUT_DIR}/python-imports.txt" || true
    
    log_info "Python SBOM generated"
}

# Generate kernel module SBOM
generate_kernel_sbom() {
    log_info "Generating kernel module dependencies..."
    
    cd "${PROJECT_ROOT}"
    
    # List kernel modules
    find kernel -name "*.c" -o -name "*.h" | head -20 > "${OUTPUT_DIR}/kernel-sources.txt"
    
    # Extract kernel version requirements
    grep -r "KERNEL_VERSION" kernel/ 2>/dev/null | head -10 >> "${OUTPUT_DIR}/kernel-sources.txt" || true
    
    log_info "Kernel SBOM generated"
}

# Generate SPDX SBOM
generate_spdx() {
    log_info "Generating SPDX SBOM..."
    
    SPDX_FILE="${OUTPUT_DIR}/agnos-${VERSION}.spdx.json"
    
    cat > "${SPDX_FILE}" << EOF
{
  "spdxVersion": "SPDX-2.3",
  "dataLicense": "CC0-1.0",
  "SPDXID": "SPDXRef-DOCUMENT",
  "name": "AGNOS-${VERSION}",
  "documentNamespace": "https://github.com/agnostos/agnos/sbom/${VERSION}",
  "creationInfo": {
    "created": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
    "creators": [
      "Tool: agnos-sbom-generator-1.0",
      "Organization: AGNOS Project"
    ]
  },
  "packages": [
EOF
    
    # Add Rust packages
    cd "${PROJECT_ROOT}/userland"
    local first=true
    cargo metadata --format-version 1 2>/dev/null | python3 -c "
import sys, json
data = json.load(sys.stdin)
packages = data.get('packages', [])
for pkg in packages:
    if not pkg['name'].startswith('agnos'):
        print(f'{\"\" if first else \",\"}{{\"SPDXID\": \"SPDXRef-Package-{pkg[\\'name\\']}\", \"name\": \"{pkg[\\'name\\']}\", \"version\": \"{pkg[\\'version\\']}\", \"downloadLocation\": \"{pkg.get(\\'repository\\', \"NOASSERTION\")}\", \"licenseConcluded\": \"NOASSERTION\", \"copyrightText\": \"NOASSERTION\", \"supplier\": \"Person: {pkg.get(\\'authors\\', [\\'Unknown\\'])[0] if pkg.get(\\'authors\\') else \\\'Unknown\\\'}\"}}')
" >> "${SPDX_FILE}" 2>/dev/null || true
    
    # Close SPDX document
    cat >> "${SPDX_FILE}" << EOF

  ],
  "relationships": [
    {
      "spdxElementId": "SPDXRef-DOCUMENT",
      "relatedSpdxElement": "SPDXRef-Package-agnos",
      "relationshipType": "DESCRIBES"
    }
  ]
}
EOF
    
    log_info "SPDX SBOM generated: ${SPDX_FILE}"
}

# Generate CycloneDX SBOM
generate_cyclonedx() {
    log_info "Generating CycloneDX SBOM..."
    
    BOM_FILE="${OUTPUT_DIR}/agnos-${VERSION}-bom.json"
    
    cat > "${BOM_FILE}" << EOF
{
  "bomFormat": "CycloneDX",
  "specVersion": "1.5",
  "serialNumber": "urn:uuid:$(uuidgen 2>/dev/null || echo '00000000-0000-0000-0000-000000000000')",
  "version": 1,
  "metadata": {
    "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
    "tools": [
      {
        "vendor": "AGNOS Project",
        "name": "sbom-generator",
        "version": "1.0"
      }
    ],
    "component": {
      "type": "application",
      "name": "AGNOS",
      "version": "${VERSION}",
      "description": "AI-Native General Operating System"
    }
  },
  "components": [
EOF
    
    cd "${PROJECT_ROOT}/userland"
    # Add components from Cargo
    cargo metadata --format-version 1 2>/dev/null | python3 -c "
import sys, json
data = json.load(sys.stdin)
packages = data.get('packages', [])
components = []
for pkg in packages:
    if not pkg['name'].startswith('agnos'):
        purl = f\"pkg:cargo/{pkg['name']}@{pkg['version']}\"
        comp = {
            'type': 'library',
            'name': pkg['name'],
            'version': pkg['version'],
            'purl': purl,
            'scope': 'required'
        }
        components.append(comp)

print(json.dumps(components, indent=2)[1:-1])
" >> "${BOM_FILE}" 2>/dev/null || true
    
    # Close CycloneDX document
    cat >> "${BOM_FILE}" << EOF

  ]
}
EOF
    
    log_info "CycloneDX SBOM generated: ${BOM_FILE}"
}

# Validate SBOM
validate_sbom() {
    log_info "Validating SBOM files..."
    
    if [ -f "${OUTPUT_DIR}/agnos-${VERSION}.spdx.json" ]; then
        # Basic JSON validation
        if python3 -m json.tool "${OUTPUT_DIR}/agnos-${VERSION}.spdx.json" > /dev/null 2>&1; then
            log_info "SPDX SBOM is valid JSON"
        else
            log_error "SPDX SBOM has JSON errors"
        fi
    fi
    
    if [ -f "${OUTPUT_DIR}/agnos-${VERSION}-bom.json" ]; then
        if python3 -m json.tool "${OUTPUT_DIR}/agnos-${VERSION}-bom.json" > /dev/null 2>&1; then
            log_info "CycloneDX SBOM is valid JSON"
        else
            log_error "CycloneDX SBOM has JSON errors"
        fi
    fi
}

# Main execution
main() {
    log_info "Generating SBOM for AGNOS v${VERSION}..."
    
    generate_rust_sbom
    generate_python_sbom
    generate_kernel_sbom
    generate_spdx
    generate_cyclonedx
    validate_sbom
    
    log_info "SBOM generation complete!"
    log_info "Output directory: ${OUTPUT_DIR}"
    
    # List generated files
    ls -lh "${OUTPUT_DIR}/"
}

# Handle script arguments
case "${1:-generate}" in
    generate)
        main
        ;;
    validate)
        validate_sbom
        ;;
    clean)
        log_info "Cleaning SBOM directory..."
        rm -rf "${OUTPUT_DIR}"
        ;;
    *)
        echo "Usage: $0 {generate|validate|clean}"
        exit 1
        ;;
esac
