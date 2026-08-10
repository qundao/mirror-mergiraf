// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Foo} from "../src/Foo.sol";

contract CounterScript is Script {
    Counter public counter;

    function setUp() public {}

    function run() public {
        vm.startBroadcast();

        counter = new Counter();

        vm.stopBroadcast();
    }
}
