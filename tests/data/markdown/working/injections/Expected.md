# Tutorial

Get started with:
```python
from bookmaker import front_page, compile, table_of_contents

def main():
    compile(toc=table_of_contents, debug=True)
```

or in Java:
```java
Bookmaker bm = new Bookmaker();
bookmaker.addTOC(new TableOfContents());
bookmaker.compile(true);
```
