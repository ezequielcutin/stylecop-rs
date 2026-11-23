namespace Sample
{
    partial class Open
    {
    }

    internal class Second
    {
    }

    class SyntaxRules
    {
        public int Later;

        const int Combined = 1, Also = 2;

        readonly int AfterPlain;

        private int Plain;

        public static void AfterInstance()
        {
        }

        static SyntaxRules()
        {
        }

        public SyntaxRules() : base()
        {
        }

        public int Property
        {
            set { value = 0; }
            get { return 0; }
        }

        public event System.EventHandler Gate
        {
            remove { }
            add { }
        }

        internal protected int Flipped;

        private async static void Run()
        {
            if (true)
            // keep
            {
            }

            int value = (1);
            int mixed = 1 + 2 * 3;
            if (true || false && true)
            {
            }

            string empty = "";
            void Local<T>() where T : class { }
            int one = 1; int two = 2;
            ;
            #region Inside
            int hidden = 0;
            #endregion
        }

        #region Outside

        enum Row
        {
            A, B
        }

        #endregion

        [System.Obsolete(), System.ComponentModel.Browsable(false)] int shared;
    }

    namespace Nested
    {
    }

    class Order
    {
        private void Hidden()
        {
        }

        public void Visible()
        {
        }

        int plain;

        const int Value = 1;

        int other;

        readonly int later;

        void Instance()
        {
        }

        static void Static()
        {
        }
    }
}
