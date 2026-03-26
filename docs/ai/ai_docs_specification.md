# AI-Oriented Documentation Specification

## Core Principles

1. **High Information Density**: Avoid verbose explanations. Prioritize the use of code, tables, lists, and formal descriptions.
2. **Structured Presentation**: Documents should be easily parsable by AI. Use clear hierarchical headings and standard Markdown syntax (e.g., tables, code blocks).
3. **Closed-Loop Description**: Every feature must include both its "Declaration/Definition" and "Usage Examples."
4. **Zero Ambiguity**: Explicitly state optionality, default values, constraints, and side effects.

---

## Writing Guidelines

### 1. Module/Macro/Function Overview
Each major unit should start with a one-sentence summary definition, followed by an outline of its core capabilities.

### 2. Declarations & Parameters
Macros or complex functions should provide a formal syntax structure or type declaration.

**Formatting Requirements:**
- Use code blocks for declarations.
- Parameter descriptions should include: Name, Type, Optionality, Default Value, and Purpose.

### 3. Usage Examples
Every feature must be accompanied by a Minimal Working Example (MWE).

**Formatting Requirements:**
- Distinguish between success paths and error paths.
- Annotate the generated code (specifically for macros).

### 4. Attributes & Metadata
For procedural macros, list all supported attributes in detail.

**Table Format:**
| Attribute | Scope | Parameters | Description |
| :--- | :--- | :--- | :--- |
| `#[display]` | Variant | `"template"` | Sets the rendering template |

### 5. Advanced Patterns
List non-obvious but high-value usage combinations, generic constraints, or collaborations with other libraries.

---

## Example Template (AI-Reference Format)

### `example_macro!` (Macro)
**Declaration:**
```rust
macro_rules! example {
    ($name:ident = { ... }) => { ... }
}
```

**Supported Attributes:**
- `#[attr1(arg)]`: Applied to...

**Usage:**
```rust
example! { Name = { ... } }
```
