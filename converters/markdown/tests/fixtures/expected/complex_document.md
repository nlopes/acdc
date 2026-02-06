# Complex Document Title

This is the preamble with an introduction paragraph.

## Introduction

This section contains **bold**, *italic*, and `monospace` text.

Here's a link to [Rust](https://rust-lang.org/) and an email [contact](mailto:mailto:info@example.com).

### Code Example

```
`fn fibonacci(n: u32) -> u32 {
    match n {
        0 => 0,
        1 => 1,
        _ => fibonacci(n - 1) + fibonacci(n - 2),
    }
}````

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
| --- | --- | --- |
| Rust | Systems | 2015 |
| Python | Scripting | 1991 |

## Advanced Features

### Blockquote

> Don't Panic.


### Admonition

<!-- Warning: Tip admonitions not natively supported in Markdown, using blockquote with label -->
> **Tip**
> Always write tests for your code!


### Images

![System Diagram](diagram.png)

Inline image:![image](icon.png) in text.

## Conclusion

This document demonstrates various AsciiDoc features and their Markdown conversion.

Some features like H<sub>2</sub>O (subscript) and E=mc<sup>2</sup> (superscript) use HTML tags in Markdown.


