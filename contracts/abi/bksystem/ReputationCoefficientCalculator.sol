/*
 * Copyright (c) GOSH Technology Ltd. All rights reserved.
 * 
 * Acki Nacki and GOSH are either registered trademarks or trademarks of GOSH
 * 
 * Licensed under the ANNL. See License.txt in the project root for license information.
*/
pragma gosh-solidity >=0.76.1;
pragma AbiHeader expire;
pragma AbiHeader pubkey;

contract AckiNackiBlockKeeperNodeWallet {
    string constant version = "1.0.0";

    constructor (
    ) {
    }

    //Getters
    function calcRepCoef(uint128 reptime) external pure returns(uint128) {
        return gosh.calcrepcoef(reptime);
    }
}
