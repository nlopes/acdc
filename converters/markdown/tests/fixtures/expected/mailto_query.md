# mailto-query(1)

<a id="_name"></a>
## Name

mailto-query - email subjects and bodies

<a id="_synopsis"></a>
## Synopsis

Email links.

<a id="_description"></a>
## Description

[Subject](mailto:subject@example.org?subject=Test%20subject)

[Both](mailto:both@example.org?subject=Test%20subject&body=Message%20body)

[fallback@example.org](mailto:fallback@example.org?subject=Test%20subject&body=Message%20body)

[Body](mailto:body@example.org?subject=&body=Message%20body)

[Empty](mailto:empty@example.org?subject=&body=)

[Quoted](mailto:quoted@example.org?subject=Hello%2C%20world&body=Say%20%22hello%22%2C%20please)

[Unicode](mailto:unicode@example.org?subject=Caf%C3%A9%20%26%20tea%3F%20%2B%2050%25%20%231%20~&body=x%3Dy%20%2F%20caf%C3%A9)

[Named](mailto:named@example.org)

[Named only](mailto:named-only@example.org)

[Query](mailto:query@example.org?cc=copy@example.org&subject=New%20subject&body=New%20body)

[Escape](mailto:escape@example.org?subject=Subject%5D&body=Body%5D)

[Control](mailto:control@example.org)

[Trailing](mailto:trailing@example.org?subject=)

[Unquoted](mailto:unquoted@example.org?subject=&body=Body)

[Numeric](mailto:numeric@example.org)

[Styled](mailto:attributes@example.org?subject=Subject&body=Body)

[Single](mailto:single@example.org?subject=It%27s%20fine&body=Say%20%22hello%22)

[Pass argument](mailto:passarg@example.org?subject=one%2Ctwo&body=%2B)

[empty-target@example.org](mailto:empty-target@example.org)

[Plain text](mailto:plain@example.org?subject=%2ABold%2A%20%3Ctag%3E%20%28C%29&body=a%20-%3E%20b)

mailto:literal@example.org\[Literal,Subject,Body\]
