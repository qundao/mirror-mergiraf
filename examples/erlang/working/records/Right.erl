-module(test).

-record(test, {
    x,
    g :: binary(),
    h :: integer() = 0,
    b :: atom()
}).

main() ->
    X = #test{g = <<"test">>, x = 1, b = ok},
    X.
