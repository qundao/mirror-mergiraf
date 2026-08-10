-module(test).

-record(test, {
    g :: binary(),
    h :: integer() = 0
}).

main() ->
    X = #test{g = <<"test">>},
    X.
