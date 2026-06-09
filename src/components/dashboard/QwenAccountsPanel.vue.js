/// <reference types="G:/Tools/RustProxyHub/.toolchain/node_modules/@vue/language-core/types/template-helpers.d.ts" />
/// <reference types="G:/Tools/RustProxyHub/.toolchain/node_modules/@vue/language-core/types/props-fallback.d.ts" />
const store = useStore();
function sessionOpen(accountId) {
    return store.overview?.open_qwen_account_login_sessions.includes(accountId) ?? false;
}
const __VLS_ctx = {
    ...{},
    ...{},
};
let __VLS_components;
let __VLS_intrinsics;
let __VLS_directives;
__VLS_asFunctionalElement1(__VLS_intrinsics.section, __VLS_intrinsics.section)({
    ...{ class: "panel account-panel" },
});
/** @type {__VLS_StyleScopedClasses['panel']} */ ;
/** @type {__VLS_StyleScopedClasses['account-panel']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
    ...{ class: "panel-top" },
});
/** @type {__VLS_StyleScopedClasses['panel-top']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({});
__VLS_asFunctionalElement1(__VLS_intrinsics.p, __VLS_intrinsics.p)({
    ...{ class: "eyebrow" },
});
/** @type {__VLS_StyleScopedClasses['eyebrow']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.h2, __VLS_intrinsics.h2)({});
__VLS_asFunctionalElement1(__VLS_intrinsics.p, __VLS_intrinsics.p)({
    ...{ class: "panel-copy" },
});
/** @type {__VLS_StyleScopedClasses['panel-copy']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.span, __VLS_intrinsics.span)({
    ...{ class: "status-chip" },
    'data-state': "accent",
});
/** @type {__VLS_StyleScopedClasses['status-chip']} */ ;
(__VLS_ctx.store.qwenAccounts.length);
__VLS_asFunctionalElement1(__VLS_intrinsics.form, __VLS_intrinsics.form)({
    ...{ onSubmit: (...[$event]) => {
            __VLS_ctx.store.addQwenAccount();
            // @ts-ignore
            [store, store,];
        } },
    ...{ class: "account-form" },
});
/** @type {__VLS_StyleScopedClasses['account-form']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.label, __VLS_intrinsics.label)({
    ...{ class: "field" },
});
/** @type {__VLS_StyleScopedClasses['field']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.span, __VLS_intrinsics.span)({});
__VLS_asFunctionalElement1(__VLS_intrinsics.input)({
    type: "email",
    placeholder: "operator@domain.com",
});
(__VLS_ctx.store.qwenEmail);
__VLS_asFunctionalElement1(__VLS_intrinsics.label, __VLS_intrinsics.label)({
    ...{ class: "field" },
});
/** @type {__VLS_StyleScopedClasses['field']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.span, __VLS_intrinsics.span)({});
__VLS_asFunctionalElement1(__VLS_intrinsics.input)({
    type: "password",
    placeholder: "optional for seeded login",
});
(__VLS_ctx.store.qwenPassword);
__VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
    ...{ class: "action-row" },
});
/** @type {__VLS_StyleScopedClasses['action-row']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.button, __VLS_intrinsics.button)({
    ...{ class: "primary-button" },
    type: "submit",
    disabled: (__VLS_ctx.store.isBusy('qwen-account:add')),
});
/** @type {__VLS_StyleScopedClasses['primary-button']} */ ;
(__VLS_ctx.store.isBusy('qwen-account:add') ? 'Saving...' : 'Save account');
__VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
    ...{ class: "account-list" },
});
/** @type {__VLS_StyleScopedClasses['account-list']} */ ;
for (const [account] of __VLS_vFor((__VLS_ctx.store.filteredQwenAccounts))) {
    __VLS_asFunctionalElement1(__VLS_intrinsics.article, __VLS_intrinsics.article)({
        key: (account.id),
        ...{ class: "account-card" },
    });
    /** @type {__VLS_StyleScopedClasses['account-card']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
        ...{ class: "dossier-index" },
    });
    /** @type {__VLS_StyleScopedClasses['dossier-index']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
        ...{ class: "panel-top" },
    });
    /** @type {__VLS_StyleScopedClasses['panel-top']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({});
    __VLS_asFunctionalElement1(__VLS_intrinsics.p, __VLS_intrinsics.p)({
        ...{ class: "eyebrow" },
    });
    /** @type {__VLS_StyleScopedClasses['eyebrow']} */ ;
    (account.id);
    __VLS_asFunctionalElement1(__VLS_intrinsics.h3, __VLS_intrinsics.h3)({});
    (account.email);
    __VLS_asFunctionalElement1(__VLS_intrinsics.p, __VLS_intrinsics.p)({
        ...{ class: "panel-copy" },
    });
    /** @type {__VLS_StyleScopedClasses['panel-copy']} */ ;
    (account.has_password ? 'Password seeded' : 'Manual browser login only');
    __VLS_asFunctionalElement1(__VLS_intrinsics.span, __VLS_intrinsics.span)({
        ...{ class: "status-chip" },
        'data-state': (__VLS_ctx.sessionOpen(account.id) ? 'healthy' : 'idle'),
    });
    /** @type {__VLS_StyleScopedClasses['status-chip']} */ ;
    (__VLS_ctx.sessionOpen(account.id) ? 'profile open' : 'saved');
    __VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
        ...{ class: "action-row" },
    });
    /** @type {__VLS_StyleScopedClasses['action-row']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.button, __VLS_intrinsics.button)({
        ...{ onClick: (...[$event]) => {
                __VLS_ctx.store.startQwenAccountLogin(account.id);
                // @ts-ignore
                [store, store, store, store, store, store, sessionOpen, sessionOpen,];
            } },
        ...{ class: "ghost-button" },
        disabled: (__VLS_ctx.store.isBusy(`login:qwen-account:start:${account.id}`)),
    });
    /** @type {__VLS_StyleScopedClasses['ghost-button']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.button, __VLS_intrinsics.button)({
        ...{ onClick: (...[$event]) => {
                __VLS_ctx.store.stopQwenAccountLogin(account.id);
                // @ts-ignore
                [store, store,];
            } },
        ...{ class: "secondary-button" },
        disabled: (!__VLS_ctx.sessionOpen(account.id)),
    });
    /** @type {__VLS_StyleScopedClasses['secondary-button']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.button, __VLS_intrinsics.button)({
        ...{ onClick: (...[$event]) => {
                __VLS_ctx.store.removeQwenAccount(account.id);
                // @ts-ignore
                [store, sessionOpen,];
            } },
        ...{ class: "danger-button" },
        disabled: (__VLS_ctx.store.isBusy(`qwen-account:remove:${account.id}`)),
    });
    /** @type {__VLS_StyleScopedClasses['danger-button']} */ ;
    // @ts-ignore
    [store,];
}
if (!__VLS_ctx.store.filteredQwenAccounts.length) {
    __VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
        ...{ class: "info-card" },
    });
    /** @type {__VLS_StyleScopedClasses['info-card']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.p, __VLS_intrinsics.p)({
        ...{ class: "info-label" },
    });
    /** @type {__VLS_StyleScopedClasses['info-label']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.p, __VLS_intrinsics.p)({
        ...{ class: "mono-line" },
    });
    /** @type {__VLS_StyleScopedClasses['mono-line']} */ ;
}
// @ts-ignore
[store,];
const __VLS_export = (await import('vue')).defineComponent({});
export default {};
