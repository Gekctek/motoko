import { debugPrint } = "mo:⛔";
let (#a a2l) or (#b a2l) : { #a : Nat; #b : Nat } = #a 5;
let (#a a2r) or (#b a2r) : { #a : Nat; #b : Nat } = #b 7;
// (should work, fails currently)
let #a a4 or #b a4 = #a 5;
let #a a5 or #b (a5 : Int) = #b 7;

let (#a a6 or #b a6, _) = (#a 5, "Ignore");

debugPrint (debug_show {a2l; a2r; a4; a5; a6});

// should reject
func foo(#a (a7 : Nat) or #b (a7 : Int)) : Nat = a7;

// should reject
func foo(#a (a7 : Nat) or #b (a7 : Int), b7 : Char) : Nat = a7;
// HEH:
// > foo (#b (-5), 'H');
// -5 : Nat


// should accept
func foo(#a (a8 : Nat) or #b a8) : Nat = a8;
