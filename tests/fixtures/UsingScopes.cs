global using Zulu;
using Alpha;
#if DEBUG
using Zebra;
using Apple;
#endif
using Omega;

namespace Outer
{
    using global::System;
    using System.Linq;

    namespace Inner
    {
        using Zeta;
        using Alpha;

        class C
        {
            void M()
            {
                using var disposable = GetResource();
            }
        }
    }
}

namespace Collation
{
    using zeta;
    using Zeta;
    using Zoo;
    using zebra;
    using Åland;
    using Aland;
}
