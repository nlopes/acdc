# Checklist contexts

## Nested checklist states

- [ ] Unchecked parent
    - [x] Lowercase checked child
    - [x] Uppercase checked child
    - [x] Star checked child

- [x] Checked sibling

## Checklist nested under an ordered item

1. Ordered parent
    - [ ] Unchecked child
        - [x] Checked grandchild

    - [x] Checked sibling

2. Ordered sibling

## Ordered list nested under a checklist item

- [x] Checklist parent
    1. Ordered child
        1. \[x\] Literal lowercase marker
        2. \[X\] Literal uppercase marker
        3. \[\*\] Literal star marker
        4. \[ \] Literal unchecked marker


- [ ] Checklist sibling

## Ordinary bullet nested under a checklist item

- [x] Checklist parent
    - Ordinary bullet
        - [ ] Nested checklist



## Checklist-like markers in an ordered list

1. \[ \] Literal unchecked marker
2. \[x\] Literal lowercase marker
3. \[X\] Literal uppercase marker
4. \[\*\] Literal star marker


