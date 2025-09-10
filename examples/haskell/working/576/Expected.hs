<<<<<<< LEFT
foo :: (A') => Foo
foo = Foo
      {
        -- comment
        bar = 0
      }
||||||| BASE
foo :: (A) => Foo
foo = Foo
      {
        -- comment
        bar = 0
      }
=======
foo :: (A) => Foo
foo = Foo
      {
        bar = 0
      }
>>>>>>> RIGHT
