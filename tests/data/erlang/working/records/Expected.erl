-module(test).

-record(test, {
    a = undefined,
    b :: atom(),
    g :: binary(),
    h :: integer() = 0,
    x
}).

main() ->
    X = #test{g = <<"test">>, a = 2, b = ok, x = 1},
    X.
