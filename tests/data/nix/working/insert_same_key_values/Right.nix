{
  pkgs,
  options,
}:

{
  foo.bar = "Hello World";
  bar = "New Key!";
  foo.baz = "Hello World";

  foo.foo = "Hello World";
}
