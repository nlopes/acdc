# code-brace-escapes(1)

2026-10-02

<a id="_name"></a>
## NAME

code-brace-escapes - preserve escaped references in code

<a id="_description"></a>
## DESCRIPTION

```
C01 \{value} / {value\} / \{value\} / {value}.
C02 α {value\} β \{value\} γ.
C03 \{markup} / {markup\} / \{markup\}.
C04 \\{value} / \\{value\} / \\\{value\}.
C05 {value\\} / \{value\\} / {value\\\}.
C06 \{bad name} / {bad name\} / \{bad name\}.
```

```
C07 \{value} / {value\} / \{value\} / {value}.
```

```rust
C08 let template = "{value\} / \{value\} / {value}";
```

```
C09 *{value\}* / https://example.org[\{value\}].
```

```
C10 \{value} / {value\} / \{value\} / {value}.
```

```
C11 \{value} / {value\} / \{value\} / {value}.
```

```
C12 *{value\}* / *\{value\}* / *{value}*.
```

```
C13 \{_value_} / {_value_\} / \{_value_\}.
```
