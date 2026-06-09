/// <reference types="G:/Tools/RustProxyHub/.toolchain/node_modules/@vue/language-core/types/template-helpers.d.ts" />
/// <reference types="G:/Tools/RustProxyHub/.toolchain/node_modules/@vue/language-core/types/props-fallback.d.ts" />
const __VLS_props = defineProps();
const store = useStore();
const providerTitles = {
    qwen: 'Qwen account bank',
    deepseek: 'DeepSeek bridge',
    kimi: 'Kimi auto-continue',
    chatgpt: 'ChatGPT browser session',
    gemini: 'Gemini browser session',
};
const providerNotes = {
    qwen: 'Rotation, uploads, stop control, and prefixed hub models.',
    deepseek: 'Reasoning-heavy browser proxy with normalized search flag support.',
    kimi: 'Pause-aware browser proxy with explicit unsupported-search warnings.',
    chatgpt: 'Manual Playwright login, live model discovery, and bridged completions.',
    gemini: 'Manual Playwright login, live model discovery, and bridged completions.',
};
function loginOpen(provider) {
    return store.overview?.open_provider_login_sessions.includes(provider) ?? false;
}
function statusTone(provider) {
    if (!provider.running)
        return 'idle';
    if (provider.login_state === 'authenticated')
        return 'healthy';
    if (provider.health_status === 'ok')
        return 'running';
    if (provider.health_status === 'degraded')
        return 'degraded';
    return 'running';
}
function formatStarted(value) {
    if (!value)
        return 'n/a';
    return new Date(value * 1000).toLocaleString();
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
    ...{ class: "panel providers-panel" },
});
/** @type {__VLS_StyleScopedClasses['panel']} */ ;
/** @type {__VLS_StyleScopedClasses['providers-panel']} */ ;
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
    'data-state': (__VLS_ctx.providers.length ? 'accent' : 'idle'),
});
/** @type {__VLS_StyleScopedClasses['status-chip']} */ ;
(__VLS_ctx.providers.length);
__VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
    ...{ class: "provider-grid provider-grid-expanded" },
});
/** @type {__VLS_StyleScopedClasses['provider-grid']} */ ;
/** @type {__VLS_StyleScopedClasses['provider-grid-expanded']} */ ;
for (const [provider] of __VLS_vFor((__VLS_ctx.providers))) {
    __VLS_asFunctionalElement1(__VLS_intrinsics.article, __VLS_intrinsics.article)({
        key: (provider.name),
        ...{ class: "provider-panel" },
    });
    /** @type {__VLS_StyleScopedClasses['provider-panel']} */ ;
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
    (__VLS_ctx.providerTitles[provider.name]);
    __VLS_asFunctionalElement1(__VLS_intrinsics.p, __VLS_intrinsics.p)({
        ...{ class: "panel-copy" },
    });
    /** @type {__VLS_StyleScopedClasses['panel-copy']} */ ;
    (__VLS_ctx.providerNotes[provider.name]);
    __VLS_asFunctionalElement1(__VLS_intrinsics.span, __VLS_intrinsics.span)({
        ...{ class: "status-chip" },
        'data-state': (__VLS_ctx.statusTone(provider)),
    });
    /** @type {__VLS_StyleScopedClasses['status-chip']} */ ;
    (provider.login_state.replaceAll('_', ' '));
    __VLS_asFunctionalElement1(__VLS_intrinsics.dl, __VLS_intrinsics.dl)({
        ...{ class: "facts" },
    });
    /** @type {__VLS_StyleScopedClasses['facts']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({});
    __VLS_asFunctionalElement1(__VLS_intrinsics.dt, __VLS_intrinsics.dt)({});
    __VLS_asFunctionalElement1(__VLS_intrinsics.dd, __VLS_intrinsics.dd)({});
    (provider.health_status);
    __VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({});
    __VLS_asFunctionalElement1(__VLS_intrinsics.dt, __VLS_intrinsics.dt)({});
    __VLS_asFunctionalElement1(__VLS_intrinsics.dd, __VLS_intrinsics.dd)({});
    (provider.model_count);
    __VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({});
    __VLS_asFunctionalElement1(__VLS_intrinsics.dt, __VLS_intrinsics.dt)({});
    __VLS_asFunctionalElement1(__VLS_intrinsics.dd, __VLS_intrinsics.dd)({});
    (__VLS_ctx.formatStarted(provider.started_at));
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
    (provider.base_url);
    __VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
        ...{ class: "provider-meta-line" },
    });
    /** @type {__VLS_StyleScopedClasses['provider-meta-line']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.span, __VLS_intrinsics.span)({
        ...{ class: "mini-pill" },
        'data-state': (provider.web_search_supported ? 'healthy' : 'idle'),
    });
    /** @type {__VLS_StyleScopedClasses['mini-pill']} */ ;
    (provider.web_search_supported ? 'web search mapped' : 'web search warned');
    if (provider.last_error) {
        __VLS_asFunctionalElement1(__VLS_intrinsics.span, __VLS_intrinsics.span)({
            ...{ class: "mini-pill" },
            'data-state': "degraded",
        });
        /** @type {__VLS_StyleScopedClasses['mini-pill']} */ ;
    }
    if (__VLS_ctx.loginOpen(provider.name)) {
        __VLS_asFunctionalElement1(__VLS_intrinsics.span, __VLS_intrinsics.span)({
            ...{ class: "mini-pill" },
            'data-state': "accent",
        });
        /** @type {__VLS_StyleScopedClasses['mini-pill']} */ ;
    }
    __VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
        ...{ class: "model-cloud" },
    });
    /** @type {__VLS_StyleScopedClasses['model-cloud']} */ ;
    for (const [model] of __VLS_vFor((provider.models.slice(0, 8)))) {
        __VLS_asFunctionalElement1(__VLS_intrinsics.span, __VLS_intrinsics.span)({
            key: (`${provider.name}:${model}`),
            ...{ class: "model-chip" },
        });
        /** @type {__VLS_StyleScopedClasses['model-chip']} */ ;
        (provider.name);
        (model);
        // @ts-ignore
        [providers, providers, providers, providerTitles, providerNotes, statusTone, formatStarted, loginOpen,];
    }
    if (!provider.models.length) {
        __VLS_asFunctionalElement1(__VLS_intrinsics.span, __VLS_intrinsics.span)({
            ...{ class: "empty-chip" },
        });
        /** @type {__VLS_StyleScopedClasses['empty-chip']} */ ;
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
    (__VLS_ctx.loginOpen(provider.name) ? 'Reopen login' : 'Open login');
    __VLS_asFunctionalElement1(__VLS_intrinsics.button, __VLS_intrinsics.button)({
        ...{ onClick: (...[$event]) => {
                __VLS_ctx.store.stopProviderLogin(provider.name);
                // @ts-ignore
                [loginOpen, store, store,];
            } },
        ...{ class: "secondary-button" },
        disabled: (!__VLS_ctx.loginOpen(provider.name)),
    });
    /** @type {__VLS_StyleScopedClasses['secondary-button']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.button, __VLS_intrinsics.button)({
        ...{ onClick: (...[$event]) => {
                __VLS_ctx.store.openProviderDrawer(provider.name);
                // @ts-ignore
                [loginOpen, store,];
            } },
        ...{ class: "primary-button" },
    });
    /** @type {__VLS_StyleScopedClasses['primary-button']} */ ;
    // @ts-ignore
    [];
}
// @ts-ignore
[];
const __VLS_export = (await import('vue')).defineComponent({
    __typeProps: {},
});
export default {};
