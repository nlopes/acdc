<a id="norm:clmulh_op"></a>
<!-- Warning: description lists not natively supported in Markdown, using regular list -->
- **Operation**

```sail
let rs1_val = X(rs1);
let rs2_val = X(rs2);
let output : xlenbits = 0;

foreach (i from 1 to xlen by 1) {
   output = if ((rs2_val >> i) & 1)
            then output ^ (rs1_val >> (xlen - i));
            else output;
}

X[rd] = output
```

```sail
((visible)) (((hidden))) indexterm:[hidden] (1)
```

- **(1)** Parentheses stay literal.

```
if ((value >> shift) & 1) then f((value));
```

```
if ((value >> shift) & 1) then f((value));
```

An ordinary open block keeps visible and  index terms.
