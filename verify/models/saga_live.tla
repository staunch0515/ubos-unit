------------------------------ MODULE saga_live ------------------------------
(***************************************************************************)
(* Liveness of the saga protocol (VER-FORM-0101).                          *)
(*                                                                         *)
(* The model is saga.tla. This module exists so that the temporal          *)
(* properties Terminates, EndsCompletedOrCompensated and EndsAsDeclared    *)
(* are checked with their own configurations: they use the specification   *)
(* FairSpec (weak fairness of the coordinators and of the step processes), *)
(* and TLC checks liveness far more slowly than invariants, so             *)
(* saga_live.cfg and saga_live.ci.cfg have smaller bounds than saga.cfg.   *)
(* saga_sim.cfg is the bound of saga.cfg for the simulation of DEC-031.    *)
(***************************************************************************)
EXTENDS saga
=============================================================================
