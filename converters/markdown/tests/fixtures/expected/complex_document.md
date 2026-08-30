# Complex Document Title

By Author Name <author@example.com>
Version 1.0, 2024-01-15

This is the preamble with an introduction paragraph.

<a id="_introduction"></a>
## Introduction

This section contains **bold**, *italic*, and `monospace` text.

Here's a link to [Rust](https://rust-lang.org) and an email [contact](mailto:info@example.com).

<a id="_code_example"></a>
### Code Example

```rust
fn fibonacci(n: u32) -> u32 {
    match n {
        0 => 0,
        1 => 1,
        _ => fibonacci(n - 1) + fibonacci(n - 2),
    }
}
```

<a id="_lists_and_tables"></a>
### Lists and Tables

Unordered list:

- First item
- Second item with **bold** text
    - Nested item
- Third item

Ordered list:

1. Step one
2. Step two
3. Step three

Task list:

- [x] Completed task
- [ ] Pending task

Simple table:

| Name | Language | Year |
| :--- | :--- | :--- |
| Rust | Systems | 2015 |
| Python | Scripting | 1991 |

<a id="_advanced_features"></a>
## Advanced Features

<a id="_blockquote"></a>
### Blockquote

> Don't Panic.
> — Douglas Adams, <cite>The Hitchhiker's Guide to the Galaxy</cite>

<a id="_admonition"></a>
### Admonition

> [!TIP]
> Always write tests for your code!

<a id="_images"></a>
### Images

![System Diagram](diagram.png)

Inline image:![image](icon.png) in text.

<a id="_conclusion"></a>
## Conclusion

This document demonstrates various AsciiDoc features and their Markdown conversion.

Some features like H<sub>2</sub>O (subscript) and E=mc<sup>2</sup> (superscript) use HTML tags in Markdown.
