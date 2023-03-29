import { debugPrint } = "mo:⛔";
let (#a a2l) or (#b a2l) : { #a : Nat; #b : Nat } = #a 5;
let (#a a2r) or (#b a2r) : { #a : Nat; #b : Nat } = #b 7;
let #a a4 or #b a4 = #a 5;
let #a a5 or #b (a5 : Int) = #b 7;

let (#a a6 or #b a6, _) = (#a 5, "Ignore");

debugPrint (debug_show {a2l; a2r; a4; a5; a6});

// should accept
//func quux(#a (a9 : Nat) or #b a9) : Nat = a9;
