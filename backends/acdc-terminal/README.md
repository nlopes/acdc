# `acdc-terminal`

A simple terminal parser for `AsciiDoc` documents.

## Usage

```bash
acdc-cli --backend terminal simple.adoc
```

![Simple Document](images/simple.adoc.png)

You can also pass multiple files and it will parse and print them all.

```bash
acdc-cli --backend terminal *.adoc
```

## Examples

Here's a simple table.

![Table Example](images/table.adoc.png)

## TODO

- [] Add `syntect` for syntax highlighting in literal code blocks
