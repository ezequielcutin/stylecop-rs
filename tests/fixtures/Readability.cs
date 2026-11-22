public class Readability
{
    public void Indentation()
    {
        A();
       B();
    }

    public void EmptyComments()
    {
        //
        /* */
        int value = 1;
    }

    public int OpeningParen()
    {
        return One
            (1);
    }

    public void ClosingParen(int first,
        int second
        )
    {
    }

    public int EmptyParen()
    {
        return Two(
            );
    }

    public void Comma(int first
        , int second)
    {
    }

    public int FollowDeclaration()
    {
        return One(

            1);
    }

    public void AfterComma(int first,

        int second)
    {
    }

    public int Split()
    {
        return Three(1,
            2);
    }

    public int Mixed()
    {
        return Four(1, 2,
            3);
    }

    public int Span()
    {
        return Five(1, 2 +
            3);
    }

    static int One(int value) => value;

    static int Two() => 0;

    static int Three(int first, int second) => first + second;

    static int Four(int first, int second, int third) => first + second + third;

    static int Five(int first, int second) => first + second;
}
