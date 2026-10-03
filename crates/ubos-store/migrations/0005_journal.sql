-- Journal and metering references of processes, PH-1 step 5c (UBS-SYS-DVM-04 §5, DEC-057).
--
-- A process's journal (STD-CTX-011) and metering record (STD-CTX-005) are chunks whose
-- identifier is their content ID (DSN-DVM-630); `process_record` references them. The
-- verification record of a logic asset (STD-CTX-023) is a chunk referenced by the asset's
-- `verification` member.
ALTER TABLE process_record ADD COLUMN journal_id bytea;
ALTER TABLE process_record ADD COLUMN metering bytea;
