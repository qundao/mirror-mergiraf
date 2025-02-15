{
  pkgs,
  options,
}:

{
  foo.bar = "Hello World";

<<<<<<< LEFT
  bar = "New Key!";
||||||| BASE
=======
  bar = "Hello Mergiraf!";
>>>>>>> RIGHT
  foo.baz = "Hello World";

  foo.foo = "Hello World";
}
