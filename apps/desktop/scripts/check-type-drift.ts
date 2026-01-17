/**
 * Type drift detection between Rust and TypeScript definitions.
 *
 * Parses shared types from Rust source files and compares them against
 * TypeScript definitions. Reports any mismatches to catch drift early.
 *
 * Run: pnpm tsx scripts/check-type-drift.ts
 */

/* eslint-disable no-console */

import * as fs from 'fs'
import * as path from 'path'

// Paths relative to apps/desktop (resolved from script location)
const RUST_FILES = [
    'src-tauri/src/file_system/operations.rs',
    'src-tauri/src/file_system/write_operations.rs',
    'src-tauri/src/file_system/volume_manager.rs',
    'src-tauri/src/network/discovery.rs',
    'src-tauri/src/network/smb_client.rs',
    'src-tauri/src/network/known_shares.rs',
]
const TS_TYPES_FILE = 'src/lib/file-explorer/types.ts'

// Rust types that are internal and don't need TypeScript equivalents
const INTERNAL_RUST_TYPES = new Set([
    'StreamingListingState', // Internal state, not sent to frontend
    'CachedListing', // Internal cache structure
    'WriteOperationState', // Internal state tracking
    'ConflictResolutionResponse', // Internal response handling
    'KnownSharesStore', // Internal storage wrapper
    'ExtendedMetadata', // Used internally, separate API for fetching
    // Note: ListingStatus has a known TypeScript/Rust mismatch that works due to
    // how the frontend handles it. The Rust uses tag="status" but TS uses plain strings.
    // TODO: Investigate if this should be aligned or if current behavior is intentional.
    'ListingStatus',
])

// Type mapping from Rust to TypeScript
const RUST_TO_TS_TYPE: Record<string, string> = {
    String: 'string',
    str: 'string',
    bool: 'boolean',
    u8: 'number',
    u16: 'number',
    u32: 'number',
    u64: 'number',
    i8: 'number',
    i16: 'number',
    i32: 'number',
    i64: 'number',
    f32: 'number',
    f64: 'number',
    usize: 'number',
    isize: 'number',
}

interface RustField {
    name: string
    rustType: string
    tsType: string
    optional: boolean
}

interface RustStruct {
    name: string
    fields: RustField[]
    serdeRenameAll?: string
}

interface RustEnumVariant {
    name: string
    fields?: RustField[]
}

interface RustEnum {
    name: string
    variants: RustEnumVariant[]
    serdeRenameAll?: string
    serdeTag?: string
}

interface TsField {
    name: string
    type: string
    optional: boolean
}

interface TsInterface {
    name: string
    fields: TsField[]
}

interface TsTypeAlias {
    name: string
    definition: string
}

type DriftSeverity = 'error' | 'warning'

interface DriftError {
    type: 'missing_ts_type' | 'missing_rust_type' | 'field_mismatch' | 'type_mismatch' | 'variant_mismatch'
    severity: DriftSeverity
    rustType?: string
    tsType?: string
    field?: string
    expected?: string
    actual?: string
    message: string
}

/**
 * Converts Rust naming convention to TypeScript based on serde rename rules.
 */
function rustNameToTs(name: string, renameAll?: string): string {
    if (!renameAll || renameAll === 'camelCase') {
        // snake_case to camelCase
        return name.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase())
    }
    if (renameAll === 'snake_case') {
        // Already snake_case, keep as-is
        return name
    }
    return name
}

/**
 * Converts a Rust type to its expected TypeScript equivalent.
 */
function rustTypeToTs(rustType: string): { tsType: string; optional: boolean } {
    // Handle Option<T>
    const optionMatch = rustType.match(/^Option<(.+)>$/)
    if (optionMatch) {
        const inner = rustTypeToTs(optionMatch[1])
        return { tsType: inner.tsType, optional: true }
    }

    // Handle Vec<T>
    const vecMatch = rustType.match(/^Vec<(.+)>$/)
    if (vecMatch) {
        const inner = rustTypeToTs(vecMatch[1])
        return { tsType: `${inner.tsType}[]`, optional: false }
    }

    // Handle HashMap<K, V> - simplified, assumes string keys
    const hashMapMatch = rustType.match(/^HashMap<\s*String\s*,\s*(.+)>$/)
    if (hashMapMatch) {
        const inner = rustTypeToTs(hashMapMatch[1])
        return { tsType: `Record<string, ${inner.tsType}>`, optional: false }
    }

    // Handle references
    if (rustType.startsWith('&')) {
        return rustTypeToTs(rustType.slice(1).trim())
    }

    // Handle primitive types
    if (RUST_TO_TS_TYPE[rustType]) {
        return { tsType: RUST_TO_TS_TYPE[rustType], optional: false }
    }

    // Assume it's a custom type that should exist in TS with the same name
    return { tsType: rustType, optional: false }
}

/**
 * Parses Rust struct definitions with serde attributes.
 */
function parseRustStructs(content: string): RustStruct[] {
    const structs: RustStruct[] = []

    // Match structs with #[derive(...Serialize...)] or #[derive(...Deserialize...)]
    const structPattern =
        /(?:#\[derive\([^\]]*(?:Serialize|Deserialize)[^\]]*\)\]\s*)*(?:#\[serde\(([^\]]*)\)\]\s*)*pub\s+struct\s+(\w+)\s*\{([^}]+)\}/g

    let match
    while ((match = structPattern.exec(content)) !== null) {
        const serdeAttrs = match[1] || ''
        const structName = match[2]
        const fieldsBlock = match[3]

        // Check for rename_all attribute
        const renameAllMatch = serdeAttrs.match(/rename_all\s*=\s*"(\w+)"/)
        const serdeRenameAll = renameAllMatch ? renameAllMatch[1] : 'camelCase'

        // Parse fields with their preceding serde attributes
        const fields: RustField[] = []

        // Split fieldsBlock into lines and process each field with its attributes
        const lines = fieldsBlock.split('\n')
        let currentSerdeAttrs = ''

        for (const line of lines) {
            const trimmed = line.trim()

            // Check for serde attribute on field
            const serdeAttrMatch = trimmed.match(/#\[serde\(([^\]]+)\)\]/)
            if (serdeAttrMatch) {
                currentSerdeAttrs = serdeAttrMatch[1]
                continue
            }

            // Check for field definition
            const fieldMatch = trimmed.match(/^pub\s+(\w+)\s*:\s*([^,]+),?/)
            if (fieldMatch) {
                const fieldName = fieldMatch[1]
                const rustType = fieldMatch[2].trim()
                const { tsType, optional } = rustTypeToTs(rustType)

                // Check if field has #[serde(default)] - makes it optional for deserialization
                const hasSerdeDefault =
                    currentSerdeAttrs.includes('default') || currentSerdeAttrs.includes('skip_serializing_if')

                fields.push({
                    name: rustNameToTs(fieldName, serdeRenameAll),
                    rustType,
                    tsType,
                    optional: optional || hasSerdeDefault,
                })

                currentSerdeAttrs = '' // Reset for next field
            }
        }

        if (fields.length > 0) {
            structs.push({
                name: structName,
                fields,
                serdeRenameAll,
            })
        }
    }

    return structs
}

/**
 * Parses Rust enum definitions with serde attributes.
 */
function parseRustEnums(content: string): RustEnum[] {
    const enums: RustEnum[] = []

    // Match enums with serde attributes
    const enumPattern =
        /(?:#\[derive\([^\]]*(?:Serialize|Deserialize)[^\]]*\)\]\s*)*(?:#\[serde\(([^\]]*)\)\]\s*)*pub\s+enum\s+(\w+)\s*\{([^}]+)\}/g

    let match
    while ((match = enumPattern.exec(content)) !== null) {
        const serdeAttrs = match[1] || ''
        const enumName = match[2]
        const variantsBlock = match[3]

        // Check for tag and rename_all attributes
        const tagMatch = serdeAttrs.match(/tag\s*=\s*"(\w+)"/)
        const renameAllMatch = serdeAttrs.match(/rename_all\s*=\s*"(\w+)"/)

        const serdeTag = tagMatch ? tagMatch[1] : undefined
        const serdeRenameAll = renameAllMatch ? renameAllMatch[1] : 'camelCase'

        // Parse variants
        const variants: RustEnumVariant[] = []

        // Simple variants (no fields) or variants with named fields
        const lines = variantsBlock.split('\n')
        for (const line of lines) {
            const trimmed = line.trim()
            if (!trimmed || trimmed.startsWith('//') || trimmed.startsWith('#')) continue

            // Match variant with fields: VariantName { field: Type, ... }
            const withFieldsMatch = trimmed.match(/^(\w+)\s*\{\s*([^}]*)\}/)
            if (withFieldsMatch) {
                const variantName = withFieldsMatch[1]
                const fieldsStr = withFieldsMatch[2]
                const fields: RustField[] = []

                const fieldPattern = /(\w+)\s*:\s*([^,}]+)/g
                let fieldMatch
                while ((fieldMatch = fieldPattern.exec(fieldsStr)) !== null) {
                    const fieldName = fieldMatch[1]
                    const rustType = fieldMatch[2].trim()
                    const { tsType, optional } = rustTypeToTs(rustType)
                    fields.push({
                        name: rustNameToTs(fieldName, serdeRenameAll),
                        rustType,
                        tsType,
                        optional,
                    })
                }

                variants.push({ name: variantName, fields })
                continue
            }

            // Match simple variant: VariantName or VariantName,
            const simpleMatch = trimmed.match(/^(\w+)\s*,?$/)
            if (simpleMatch) {
                variants.push({ name: simpleMatch[1] })
            }
        }

        if (variants.length > 0) {
            enums.push({
                name: enumName,
                variants,
                serdeRenameAll,
                serdeTag,
            })
        }
    }

    return enums
}

/**
 * Parses TypeScript interface definitions.
 */
function parseTsInterfaces(content: string): TsInterface[] {
    const interfaces: TsInterface[] = []

    // Match interface definitions
    const interfacePattern = /export\s+interface\s+(\w+)\s*\{([^}]+)\}/g

    let match
    while ((match = interfacePattern.exec(content)) !== null) {
        const name = match[1]
        const fieldsBlock = match[2]

        const fields: TsField[] = []
        // Match field: name?: type or name: type
        const fieldPattern = /(\w+)(\?)?:\s*([^;\n]+)/g
        let fieldMatch
        while ((fieldMatch = fieldPattern.exec(fieldsBlock)) !== null) {
            // Skip JSDoc comments
            if (fieldMatch[0].includes('/**') || fieldMatch[0].includes('*/')) continue

            fields.push({
                name: fieldMatch[1],
                optional: fieldMatch[2] === '?',
                type: fieldMatch[3].trim(),
            })
        }

        interfaces.push({ name, fields })
    }

    return interfaces
}

/**
 * Parses TypeScript type alias definitions (for union types like enums).
 */
// eslint-disable-next-line complexity
function parseTsTypeAliases(content: string): TsTypeAlias[] {
    const aliases: TsTypeAlias[] = []

    // Split by "export type" to handle multi-line definitions properly
    const typeBlocks = content.split(/(?=export\s+type\s+)/)

    for (const block of typeBlocks) {
        // Match "export type Name = ..."
        const headerMatch = block.match(/^export\s+type\s+(\w+)\s*=\s*/)
        if (!headerMatch) continue

        const name = headerMatch[1]

        // Find the definition - everything after "=" until we hit another export or end
        const afterEquals = block.slice(headerMatch[0].length)

        // Find the end of this type definition
        // It ends at the next "export" keyword (that's not in a string)
        // or at a clear statement boundary
        let definition = ''
        let depth = 0 // Track brace/paren depth
        let inString = false
        let stringChar = ''

        for (let i = 0; i < afterEquals.length; i++) {
            const char = afterEquals[i]
            const prevChar = i > 0 ? afterEquals[i - 1] : ''

            // Track string state
            if ((char === '"' || char === "'") && prevChar !== '\\') {
                if (!inString) {
                    inString = true
                    stringChar = char
                } else if (char === stringChar) {
                    inString = false
                }
            }

            if (!inString) {
                if (char === '{' || char === '(' || char === '<') depth++
                if (char === '}' || char === ')' || char === '>') depth--

                // Check for end of type definition
                // End at "export" when at depth 0
                if (depth === 0 && afterEquals.slice(i).match(/^\s*(export|\/\*\*)/)) {
                    break
                }
            }

            definition += char
        }

        // Clean up the definition
        definition = definition
            .replace(/\s+/g, ' ')
            .trim()
            .replace(/\s*\|\s*/g, ' | ')

        // Remove trailing comments if any
        definition = definition.replace(/\/\/.*$/, '').trim()

        aliases.push({ name, definition })
    }

    return aliases
}

/**
 * Normalizes a TypeScript type for comparison.
 */
function normalizeType(type: string): string {
    return type.replace(/\s+/g, ' ').trim()
}

/**
 * Compares a Rust struct against a TypeScript interface.
 */
function compareStructToInterface(rust: RustStruct, ts: TsInterface): DriftError[] {
    const errors: DriftError[] = []

    const tsFieldMap = new Map(ts.fields.map((f) => [f.name, f]))

    for (const rustField of rust.fields) {
        const tsField = tsFieldMap.get(rustField.name)

        if (!tsField) {
            errors.push({
                type: 'field_mismatch',
                severity: 'error',
                rustType: rust.name,
                field: rustField.name,
                message: `Field "${rustField.name}" exists in Rust ${rust.name} but not in TypeScript`,
            })
            continue
        }

        // Check optional mismatch
        // In TypeScript, Option<T> can be represented as either:
        // - field?: T (optional field)
        // - field: T | null (required but nullable)
        // Both are acceptable representations of Rust Option<T>
        const tsHasNullUnion = tsField.type.includes('| null') || tsField.type.includes('null |')
        const rustIsOptional = rustField.optional
        const tsIsOptional = tsField.optional || tsHasNullUnion

        if (rustIsOptional !== tsIsOptional) {
            // If Rust is optional (due to serde default) but TS is required, it's usually OK
            // because the field is always present when sent from Rust.
            // This is a warning, not an error.
            const isSerdeDefaultMismatch = rustIsOptional && !tsIsOptional
            errors.push({
                type: 'field_mismatch',
                severity: isSerdeDefaultMismatch ? 'warning' : 'error',
                rustType: rust.name,
                field: rustField.name,
                expected: rustIsOptional ? 'optional' : 'required',
                actual: tsIsOptional ? 'optional' : 'required',
                message: `Field "${rustField.name}" in ${rust.name}: Rust is ${rustIsOptional ? 'optional (serde default)' : 'required'}, TypeScript is ${tsIsOptional ? 'optional (or nullable)' : 'required'}`,
            })
        }

        // Check type match (simplified - just compares normalized types)
        const expectedTsType = normalizeType(rustField.tsType)
        // Remove null from union for comparison (since we already checked optionality)
        const actualTsType = normalizeType(tsField.type.replace(/\s*\|\s*null\s*/g, '').replace(/\s*null\s*\|\s*/g, ''))

        // Allow some flexibility in type comparison
        if (!typesAreCompatible(expectedTsType, actualTsType)) {
            errors.push({
                type: 'type_mismatch',
                severity: 'error',
                rustType: rust.name,
                field: rustField.name,
                expected: expectedTsType,
                actual: actualTsType,
                message: `Field "${rustField.name}" in ${rust.name}: expected "${expectedTsType}" from Rust, got "${actualTsType}" in TypeScript`,
            })
        }
    }

    // Check for extra fields in TypeScript that don't exist in Rust
    for (const tsField of ts.fields) {
        const rustField = rust.fields.find((f) => f.name === tsField.name)
        if (!rustField) {
            errors.push({
                type: 'field_mismatch',
                severity: 'error',
                tsType: ts.name,
                field: tsField.name,
                message: `Field "${tsField.name}" exists in TypeScript ${ts.name} but not in Rust`,
            })
        }
    }

    return errors
}

/**
 * Checks if two TypeScript types are compatible.
 */
function typesAreCompatible(expected: string, actual: string): boolean {
    // Exact match
    if (expected === actual) return true

    // Handle null vs undefined for optional types
    if (expected.includes('null') && actual.includes('undefined')) return true
    if (expected.includes('undefined') && actual.includes('null')) return true

    // Handle array syntax variations
    if (expected.endsWith('[]') && actual.startsWith('Array<')) {
        const expectedInner = expected.slice(0, -2)
        const actualMatch = actual.match(/^Array<(.+)>$/)
        if (actualMatch) {
            return typesAreCompatible(expectedInner, actualMatch[1])
        }
    }

    // Handle common equivalent types
    const equivalents: Record<string, string[]> = {
        number: ['number'],
        string: ['string'],
        boolean: ['boolean'],
    }

    for (const [, types] of Object.entries(equivalents)) {
        if (types.includes(expected) && types.includes(actual)) return true
    }

    return false
}

/**
 * Converts a Rust enum variant name to its expected TypeScript value.
 */
function rustVariantToTsValue(variantName: string, renameAll?: string): string {
    if (renameAll === 'snake_case') {
        // PascalCase to snake_case
        return variantName
            .replace(/([A-Z])/g, '_$1')
            .toLowerCase()
            .replace(/^_/, '')
    }
    if (renameAll === 'camelCase') {
        // PascalCase to camelCase
        return variantName.charAt(0).toLowerCase() + variantName.slice(1)
    }
    return variantName
}

/**
 * Compares a tagged Rust enum against a TypeScript discriminated union.
 */
function compareTaggedEnumToUnion(rust: RustEnum, tsDefinition: string): DriftError[] {
    const errors: DriftError[] = []

    // Parse the union members from TS
    // Format: { type: 'variant_name'; field: string } | { type: 'other'; ... }
    const unionMembers = tsDefinition.split('|').map((m) => m.trim())

    for (const variant of rust.variants) {
        const expectedType = rustVariantToTsValue(variant.name, rust.serdeRenameAll)

        // Find matching union member
        const matchingMember = unionMembers.find((m) => m.includes(`type: '${expectedType}'`))

        if (!matchingMember) {
            errors.push({
                type: 'variant_mismatch',
                severity: 'error',
                rustType: rust.name,
                expected: expectedType,
                message: `Enum variant "${variant.name}" (serialized as "${expectedType}") not found in TypeScript ${rust.name}`,
            })
        }
    }

    return errors
}

/**
 * Compares a simple Rust enum against a TypeScript string literal union.
 */
function compareSimpleEnumToUnion(rust: RustEnum, tsDefinition: string): DriftError[] {
    const errors: DriftError[] = []

    // Parse expected values from Rust
    const expectedValues = rust.variants.map((v) => rustVariantToTsValue(v.name, rust.serdeRenameAll))

    // Parse actual values from TypeScript (e.g., "'value1' | 'value2'")
    const actualValues = tsDefinition
        .split('|')
        .map((v) => v.trim().replace(/^'|'$/g, ''))
        .filter((v) => v && !v.startsWith('{'))

    for (const expected of expectedValues) {
        if (!actualValues.includes(expected)) {
            errors.push({
                type: 'variant_mismatch',
                severity: 'error',
                rustType: rust.name,
                expected,
                message: `Enum value "${expected}" from Rust ${rust.name} not found in TypeScript`,
            })
        }
    }

    for (const actual of actualValues) {
        if (!expectedValues.includes(actual)) {
            errors.push({
                type: 'variant_mismatch',
                severity: 'error',
                tsType: rust.name,
                actual,
                message: `TypeScript ${rust.name} has value "${actual}" not in Rust enum`,
            })
        }
    }

    return errors
}

/**
 * Main function to run the drift detection.
 */
// eslint-disable-next-line complexity
function main(): void {
    // pnpm runs scripts from the package directory (apps/desktop)
    const desktopDir = process.cwd()

    // Debug: show resolved paths
    if (process.env.DEBUG) {
        console.log(`Desktop dir: ${desktopDir}`)
    }

    console.log('Checking for type drift between Rust and TypeScript...\n')

    // Read and parse Rust files
    const rustStructs: RustStruct[] = []
    const rustEnums: RustEnum[] = []

    for (const relPath of RUST_FILES) {
        const filePath = path.resolve(desktopDir, relPath)
        if (!fs.existsSync(filePath)) {
            console.log(`  Skipping ${relPath} (not found)`)
            continue
        }

        const content = fs.readFileSync(filePath, 'utf-8')
        rustStructs.push(...parseRustStructs(content))
        rustEnums.push(...parseRustEnums(content))
    }

    console.log(`Found ${String(rustStructs.length)} Rust structs and ${String(rustEnums.length)} enums\n`)

    // Read and parse TypeScript file
    const tsPath = path.resolve(desktopDir, TS_TYPES_FILE)
    const tsContent = fs.readFileSync(tsPath, 'utf-8')
    const tsInterfaces = parseTsInterfaces(tsContent)
    const tsTypeAliases = parseTsTypeAliases(tsContent)

    console.log(
        `Found ${String(tsInterfaces.length)} TypeScript interfaces and ${String(tsTypeAliases.length)} type aliases\n`,
    )

    // Create lookup maps
    const tsInterfaceMap = new Map(tsInterfaces.map((i) => [i.name, i]))
    const tsTypeAliasMap = new Map(tsTypeAliases.map((t) => [t.name, t]))

    const allErrors: DriftError[] = []

    // Compare structs to interfaces
    for (const rustStruct of rustStructs) {
        // Skip internal types that don't need TS equivalents
        if (INTERNAL_RUST_TYPES.has(rustStruct.name)) {
            continue
        }

        const tsInterface = tsInterfaceMap.get(rustStruct.name)

        if (!tsInterface) {
            // Check if it's a type alias instead (some simple structs might be)
            if (!tsTypeAliasMap.has(rustStruct.name)) {
                allErrors.push({
                    type: 'missing_ts_type',
                    severity: 'error',
                    rustType: rustStruct.name,
                    message: `Rust struct "${rustStruct.name}" has no TypeScript equivalent`,
                })
            }
            continue
        }

        const errors = compareStructToInterface(rustStruct, tsInterface)
        allErrors.push(...errors)
    }

    // Compare enums to type aliases
    for (const rustEnum of rustEnums) {
        // Skip internal types
        if (INTERNAL_RUST_TYPES.has(rustEnum.name)) {
            continue
        }

        const tsTypeAlias = tsTypeAliasMap.get(rustEnum.name)

        if (!tsTypeAlias) {
            // Some enums might be interfaces instead
            if (!tsInterfaceMap.has(rustEnum.name)) {
                allErrors.push({
                    type: 'missing_ts_type',
                    severity: 'error',
                    rustType: rustEnum.name,
                    message: `Rust enum "${rustEnum.name}" has no TypeScript equivalent`,
                })
            }
            continue
        }

        // Determine if it's a tagged enum (discriminated union) or simple enum (string literals)
        if (rustEnum.serdeTag) {
            const errors = compareTaggedEnumToUnion(rustEnum, tsTypeAlias.definition)
            allErrors.push(...errors)
        } else {
            const errors = compareSimpleEnumToUnion(rustEnum, tsTypeAlias.definition)
            allErrors.push(...errors)
        }
    }

    // Separate errors and warnings
    const errors = allErrors.filter((e) => e.severity === 'error')
    const warnings = allErrors.filter((e) => e.severity === 'warning')

    // Report results
    if (errors.length === 0 && warnings.length === 0) {
        console.log('No type drift detected between Rust and TypeScript definitions.')
        process.exit(0)
    }

    if (errors.length > 0) {
        console.log(`Found ${String(errors.length)} type drift error(s):\n`)
        for (const error of errors) {
            console.log(`  ❌ ${error.message}`)
        }
    }

    if (warnings.length > 0) {
        if (errors.length > 0) console.log()
        console.log(`Found ${String(warnings.length)} warning(s) (may be intentional):\n`)
        for (const warning of warnings) {
            console.log(`  ⚠️  ${warning.message}`)
        }
    }

    if (errors.length > 0) {
        console.log('\nPlease update the TypeScript definitions to match the Rust types,')
        console.log('or update the Rust types if the TypeScript is correct.')
        process.exit(1)
    } else {
        console.log('\nNo errors found. Warnings are informational and may be intentional design choices.')
        process.exit(0)
    }
}

main()
