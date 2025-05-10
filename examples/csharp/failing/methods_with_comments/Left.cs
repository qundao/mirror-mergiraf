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

    /// <summary>
    /// Record docs
    /// </summary>
    /// <param name="Foo">Bool arg docs</param>
    private record MyPrivateRecord(bool Foo);
}
