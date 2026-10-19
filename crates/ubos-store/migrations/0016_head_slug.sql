-- Objects by class and slug, PH-2 step 0b (DEC-087): a process resolves its URI targets
-- (`ubos://<authority>/<Class>/<slug>`), and a URI read resolves its object, from this index
-- instead of a view of the whole branch.

CREATE INDEX head_slug ON head_index (tenant_id, vae_id, branch_id, class_name, slug, cseq_to);
