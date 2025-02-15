{
  pkgs,
  options,
}:

{
  foo.bar = "Hello World";
  foo.baz = "Hello World";

  foo.foo = "Hello World";

  bar = "New Key!";
}
