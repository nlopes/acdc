# Checklist contexts

<a id="_nested_checklist_states"></a>
## Nested checklist states

- \[ \] Unchecked parent
    - \[x\] Lowercase checked child
    - \[x\] Uppercase checked child
    - \[x\] Star checked child
- \[x\] Checked sibling

<a id="_checklist_nested_under_an_ordered_item"></a>
## Checklist nested under an ordered item

1. Ordered parent
    - \[ \] Unchecked child
        - \[x\] Checked grandchild
    - \[x\] Checked sibling
2. Ordered sibling

<a id="_ordered_list_nested_under_a_checklist_item"></a>
## Ordered list nested under a checklist item

- \[x\] Checklist parent
    1. Ordered child
        1. \[x\] Literal lowercase marker
        2. \[X\] Literal uppercase marker
        3. \[\*\] Literal star marker
        4. \[ \] Literal unchecked marker
- \[ \] Checklist sibling

<a id="_ordinary_bullet_nested_under_a_checklist_item"></a>
## Ordinary bullet nested under a checklist item

- \[x\] Checklist parent
    - Ordinary bullet
        - \[ \] Nested checklist

<a id="_checklist_like_markers_in_an_ordered_list"></a>
## Checklist-like markers in an ordered list

1. \[ \] Literal unchecked marker
2. \[x\] Literal lowercase marker
3. \[X\] Literal uppercase marker
4. \[\*\] Literal star marker
