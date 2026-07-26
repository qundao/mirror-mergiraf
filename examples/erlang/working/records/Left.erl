-module(test).

-record(test, {
    a = undefined,
    b :: atom(),
    g :: binary(),
    h :: integer() = 0
}).

main() ->
    X = #test{g = <<"test">>, a = 2, b = ok},
    X.
