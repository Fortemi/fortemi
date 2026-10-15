-- #1201: global subscriptions are authorized as their creating app_user.
-- NULL preserves legacy/local subscriptions under the default archive policy.
ALTER TABLE public.webhook ADD COLUMN created_by_user_id UUID;
ALTER TABLE public.webhook ADD CONSTRAINT webhook_created_by_user_fk
    FOREIGN KEY (tenant_id, created_by_user_id)
    REFERENCES public.app_user(tenant_id, id)
    ON UPDATE RESTRICT ON DELETE RESTRICT;
CREATE INDEX webhook_created_by_user_idx ON public.webhook (tenant_id, created_by_user_id)
    WHERE created_by_user_id IS NOT NULL;
COMMENT ON COLUMN public.webhook.created_by_user_id IS
    'Verified creating app_user; current complete-catalog access is rechecked before dispatch. NULL legacy subscriptions use anonymous archive policy.';
