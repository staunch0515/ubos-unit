---------------------------- MODULE approval_live ----------------------------
(***************************************************************************)
(* Liveness of the approval and deferred-commit protocol (VER-FORM-0100).  *)
(*                                                                         *)
(* The model is approval.tla. This module exists so that the temporal      *)
(* property Terminates is checked with its own configurations: TLC's       *)
(* symmetry reduction is not sound for liveness, so approval_live.cfg and  *)
(* approval_live.ci.cfg use the specification FairSpec without SYMMETRY,   *)
(* at smaller bounds than the safety runs of approval.cfg.                 *)
(***************************************************************************)
EXTENDS approval
===============================================================================
