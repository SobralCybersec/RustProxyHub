/// <reference types="G:/Tools/RustProxyHub/.toolchain/node_modules/@vue/language-core/types/template-helpers.d.ts" />
/// <reference types="G:/Tools/RustProxyHub/.toolchain/node_modules/@vue/language-core/types/props-fallback.d.ts" />
const __VLS_props = defineProps();
const store = useStore();
const guides = {
    qwen: [
        'Open a global login for the default profile when you need fresh cookies.',
        'Use the Qwen account bank below for per-account persistent profiles.',
        'Mark the session done after the browser state is saved.',
    ],
    deepseek: [
        'Open browser login and finish sign-in on chat.deepseek.com.',
        'Wait until the chat box is usable.',
        'Mark the session done and rerun a hub smoke request.',
    ],
    kimi: [
        'Open browser login and complete sign-in on kimi.com.',
        'Leave the session long enough for the persistent profile to settle.',
        'Mark the session done when the page is ready.',
    ],
    chatgpt: [
        'Open browser login and authenticate on chatgpt.com.',
        'After login, mark the session done so the next model probe can switch headless again.',
        'Live model IDs appear on the provider card after a successful probe.',
    ],
    gemini: [
        'Open browser login and authenticate on gemini.google.com.',
        'Mark the session done after the browser state is saved.',
        'Live model IDs appear on the provider card after a successful probe.',
    ],
};
function loginOpen(provider) {
    return store.overview?.open_provider_login_sessions.includes(provider) ?? false;
}
const __VLS_ctx = {
    ...{},
    ...{},
    ...{},
    ...{},
};
let __VLS_components;
let __VLS_intrinsics;
let __VLS_directives;
__VLS_asFunctionalElement1(__VLS_intrinsics.section, __VLS_intrinsics.section)({
    ...{ class: "panel login-panel" },
});
/** @type {__VLS_StyleScopedClasses['panel']} */ ;
/** @type {__VLS_StyleScopedClasses['login-panel']} */ ;
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
(__VLS_ctx.store.openLoginCount);
__VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
    ...{ class: "login-grid" },
});
/** @type {__VLS_StyleScopedClasses['login-grid']} */ ;
for (const [provider] of __VLS_vFor((__VLS_ctx.providers))) {
    __VLS_asFunctionalElement1(__VLS_intrinsics.article, __VLS_intrinsics.article)({
        key: (provider.name),
        ...{ class: "login-card" },
    });
    /** @type {__VLS_StyleScopedClasses['login-card']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
        ...{ class: "dossier-index" },
    });
    /** @type {__VLS_StyleScopedClasses['dossier-index']} */ ;
    (provider.name.toUpperCase());
    __VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
        ...{ class: "panel-top" },
    });
    /** @type {__VLS_StyleScopedClasses['panel-top']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({});
    __VLS_asFunctionalElement1(__VLS_intrinsics.p, __VLS_intrinsics.p)({
        ...{ class: "eyebrow" },
    });
    /** @type {__VLS_StyleScopedClasses['eyebrow']} */ ;
    (provider.name);
    __VLS_asFunctionalElement1(__VLS_intrinsics.h3, __VLS_intrinsics.h3)({});
    (__VLS_ctx.store.browserPrefs[provider.name]);
    __VLS_asFunctionalElement1(__VLS_intrinsics.p, __VLS_intrinsics.p)({
        ...{ class: "panel-copy" },
    });
    /** @type {__VLS_StyleScopedClasses['panel-copy']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.span, __VLS_intrinsics.span)({
        ...{ class: "status-chip" },
        'data-state': (__VLS_ctx.loginOpen(provider.name) ? 'healthy' : 'idle'),
    });
    /** @type {__VLS_StyleScopedClasses['status-chip']} */ ;
    (__VLS_ctx.loginOpen(provider.name) ? 'window open' : 'idle');
    __VLS_asFunctionalElement1(__VLS_intrinsics.label, __VLS_intrinsics.label)({
        ...{ class: "field" },
    });
    /** @type {__VLS_StyleScopedClasses['field']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.span, __VLS_intrinsics.span)({});
    __VLS_asFunctionalElement1(__VLS_intrinsics.select, __VLS_intrinsics.select)({
        value: (__VLS_ctx.store.browserPrefs[provider.name]),
    });
    __VLS_asFunctionalElement1(__VLS_intrinsics.option, __VLS_intrinsics.option)({
        value: "msedge",
    });
    __VLS_asFunctionalElement1(__VLS_intrinsics.option, __VLS_intrinsics.option)({
        value: "chrome",
    });
    __VLS_asFunctionalElement1(__VLS_intrinsics.option, __VLS_intrinsics.option)({
        value: "chromium",
    });
    __VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
        ...{ class: "login-steps" },
    });
    /** @type {__VLS_StyleScopedClasses['login-steps']} */ ;
    for (const [step] of __VLS_vFor((__VLS_ctx.guides[provider.name]))) {
        __VLS_asFunctionalElement1(__VLS_intrinsics.p, __VLS_intrinsics.p)({
            key: (step),
            ...{ class: "step-line" },
        });
        /** @type {__VLS_StyleScopedClasses['step-line']} */ ;
        (step);
        // @ts-ignore
        [store, store, store, providers, loginOpen, loginOpen, guides,];
    }
    __VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
        ...{ class: "action-row" },
    });
    /** @type {__VLS_StyleScopedClasses['action-row']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.button, __VLS_intrinsics.button)({
        ...{ onClick: (...[$event]) => {
                __VLS_ctx.store.startProviderLogin(provider.name);
                // @ts-ignore
                [store,];
            } },
        ...{ class: "ghost-button" },
        disabled: (__VLS_ctx.store.isBusy(`login:start:${provider.name}`)),
    });
    /** @type {__VLS_StyleScopedClasses['ghost-button']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.button, __VLS_intrinsics.button)({
        ...{ onClick: (...[$event]) => {
                __VLS_ctx.store.stopProviderLogin(provider.name);
                // @ts-ignore
                [store, store,];
            } },
        ...{ class: "secondary-button" },
        disabled: (!__VLS_ctx.loginOpen(provider.name)),
    });
    /** @type {__VLS_StyleScopedClasses['secondary-button']} */ ;
    // @ts-ignore
    [loginOpen,];
}
// @ts-ignore
[];
const __VLS_export = (await import('vue')).defineComponent({
    __typeProps: {},
});
export default {};
