// Copyright header

using System;

namespace MyNamespace;

public partial class MyClass : IMyInterface
{
    /// <summary>
    /// Ctor docs
    /// </summary>
    /// <param name="ctorArg">Param docs</param>
    public MyClass(IThing1 thing1, IThing2 thing2)
    {
        _thing1 = thing1;
        _thing2 = thing2;
    }

<<<<<<< LEFT
    /// <summary>
    /// Record docs
    /// </summary>
    /// <param name="Foo">Bool arg docs</param>
||||||| BASE
=======
    /// <summary>
    /// Prop docs
    /// </summary>
    [MyCustomAttribute]
    public string? MyProp { get; set; }

>>>>>>> RIGHT
    private record MyPrivateRecord(bool Foo);
}
