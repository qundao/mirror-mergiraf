{
  pkgs,
  options,
}:

{
  foo.bar = "Hello World";
<<<<<<< LEFT
  foo.baz = "Mergiraf is fun :)";
||||||| BASE
=======
  foo.baz = "Merge Conflicts are no fun :)";
>>>>>>> RIGHT
}
