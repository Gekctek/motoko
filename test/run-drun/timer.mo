//MOC-ENV MOC_UNLOCK_PRIM=yesplease

import { debugPrint; error; setTimer; cancelTimer } = "mo:⛔";

/*
The schedule is
| seconds | reaction |
|----|-----|
| 1.0 | YEP! |
| 2.0 | DIM! |
| 3.0 | ROOK! |
| 4.0 | DIM! |
| 6.0 | DIM! |
| 8.0 | BATT! |
| 13.0 | BATT! |
| 18.0 | BATT! |
*/

actor {

  system func timer(setGlobalTimer : Nat64 -> ()) : async () {
      await @timer_helper();
  };

  var count = 0;
  var max = 8;
  let raw_rand = (actor "aaaaa-aa" : actor { raw_rand : () -> async Blob }).raw_rand;
  let second : Nat64 = 1_000_000_000;

  public shared func go() : async () {
     var attempts = 0;

     let id1 = setTimer(1 * second, false, func () : async () { count += 1; debugPrint "YEP!" });
     let id2 = setTimer(2 * second, true, func () : async () { count += 1; debugPrint "DIM!" });
     let id3 = setTimer(3 * second, false, func () : async () {
         count += 1;
         debugPrint "ROOK!";
         ignore setTimer(5 * second, true, func () : async () { count += 1; debugPrint "BATT!" })
     });

     while (count < max) {
       ignore await raw_rand(); // yield to scheduler
       attempts += 1;
       if (count >= 5) { cancelTimer id2 };
       if (attempts >= 200 and count == 0)
         throw error("he's dead Jim");
     };
     debugPrint(debug_show {count});
  };
};

//SKIP run
//SKIP run-low
//SKIP run-ir

//CALL ingress go "DIDL\x00\x00"
