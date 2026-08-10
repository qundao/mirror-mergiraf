foo_version = 1.3
foo_patch_version = $(foo_version).1
baz_image=example.com/baz:$(baz_version)-foo$(foo_version)
baz_version = 4.5.7
