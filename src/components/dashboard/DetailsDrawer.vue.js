/// <reference types="G:/Tools/RustProxyHub/.toolchain/node_modules/@vue/language-core/types/template-helpers.d.ts" />
/// <reference types="G:/Tools/RustProxyHub/.toolchain/node_modules/@vue/language-core/types/props-fallback.d.ts" />
const store = useStore();
const details = computed(() => store.activeProviderDetails);
const logs = computed(() => store.activeProviderLogs);
function prettyJson(value) {
    return JSON.stringify(value, null, 2);
}
const __VLS_ctx = {
    ...{},
    ...{},
};
let __VLS_components;
let __VLS_intrinsics;
let __VLS_directives;
if (__VLS_ctx.store.activeDrawer) {
    __VLS_asFunctionalElement1(__VLS_intrinsics.aside, __VLS_intrinsics.aside)({
        ...{ onClick: (...[$event]) => {
                if (!(__VLS_ctx.store.activeDrawer))
                    return;
                __VLS_ctx.store.closeProviderDrawer();
                // @ts-ignore
                [store, store,];
            } },
        ...{ class: "drawer-backdrop" },
    });
    /** @type {__VLS_StyleScopedClasses['drawer-backdrop']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.section, __VLS_intrinsics.section)({
        ...{ class: "drawer-panel" },
    });
    /** @type {__VLS_StyleScopedClasses['drawer-panel']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
        ...{ class: "panel-top" },
    });
    /** @type {__VLS_StyleScopedClasses['panel-top']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({});
    __VLS_asFunctionalElement1(__VLS_intrinsics.p, __VLS_intrinsics.p)({
        ...{ class: "eyebrow" },
    });
    /** @type {__VLS_StyleScopedClasses['eyebrow']} */ ;
    (__VLS_ctx.store.activeDrawer);
    __VLS_asFunctionalElement1(__VLS_intrinsics.h2, __VLS_intrinsics.h2)({});
    __VLS_asFunctionalElement1(__VLS_intrinsics.p, __VLS_intrinsics.p)({
        ...{ class: "panel-copy" },
    });
    /** @type {__VLS_StyleScopedClasses['panel-copy']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.button, __VLS_intrinsics.button)({
        ...{ onClick: (...[$event]) => {
                if (!(__VLS_ctx.store.activeDrawer))
                    return;
                __VLS_ctx.store.closeProviderDrawer();
                // @ts-ignore
                [store, store,];
            } },
        ...{ class: "secondary-button" },
    });
    /** @type {__VLS_StyleScopedClasses['secondary-button']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
        ...{ class: "drawer-stack" },
    });
    /** @type {__VLS_StyleScopedClasses['drawer-stack']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
        ...{ class: "info-card" },
    });
    /** @type {__VLS_StyleScopedClasses['info-card']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.p, __VLS_intrinsics.p)({
        ...{ class: "info-label" },
    });
    /** @type {__VLS_StyleScopedClasses['info-label']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.pre, __VLS_intrinsics.pre)({
        ...{ class: "code-window" },
    });
    /** @type {__VLS_StyleScopedClasses['code-window']} */ ;
    (__VLS_ctx.prettyJson(__VLS_ctx.details?.overview ?? {}));
    __VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
        ...{ class: "info-card" },
    });
    /** @type {__VLS_StyleScopedClasses['info-card']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.p, __VLS_intrinsics.p)({
        ...{ class: "info-label" },
    });
    /** @type {__VLS_StyleScopedClasses['info-label']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.pre, __VLS_intrinsics.pre)({
        ...{ class: "code-window" },
    });
    /** @type {__VLS_StyleScopedClasses['code-window']} */ ;
    (__VLS_ctx.prettyJson(__VLS_ctx.details?.detail ?? {}));
    __VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
        ...{ class: "info-card" },
    });
    /** @type {__VLS_StyleScopedClasses['info-card']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.p, __VLS_intrinsics.p)({
        ...{ class: "info-label" },
    });
    /** @type {__VLS_StyleScopedClasses['info-label']} */ ;
    __VLS_asFunctionalElement1(__VLS_intrinsics.pre, __VLS_intrinsics.pre)({
        ...{ class: "log-window" },
    });
    /** @type {__VLS_StyleScopedClasses['log-window']} */ ;
    (__VLS_ctx.logs.length ? __VLS_ctx.logs.join('\n') : 'No local lifecycle logs yet.');
    if (__VLS_ctx.details?.qwen_accounts?.length) {
        __VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
            ...{ class: "info-card" },
        });
        /** @type {__VLS_StyleScopedClasses['info-card']} */ ;
        __VLS_asFunctionalElement1(__VLS_intrinsics.p, __VLS_intrinsics.p)({
            ...{ class: "info-label" },
        });
        /** @type {__VLS_StyleScopedClasses['info-label']} */ ;
        __VLS_asFunctionalElement1(__VLS_intrinsics.pre, __VLS_intrinsics.pre)({
            ...{ class: "code-window" },
        });
        /** @type {__VLS_StyleScopedClasses['code-window']} */ ;
        (__VLS_ctx.prettyJson(__VLS_ctx.details.qwen_accounts));
    }
}
// @ts-ignore
[prettyJson, prettyJson, prettyJson, details, details, details, details, logs, logs,];
const __VLS_export = (await import('vue')).defineComponent({});
export default {};
