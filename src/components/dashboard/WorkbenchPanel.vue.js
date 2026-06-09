/// <reference types="G:/Tools/RustProxyHub/.toolchain/node_modules/@vue/language-core/types/template-helpers.d.ts" />
/// <reference types="G:/Tools/RustProxyHub/.toolchain/node_modules/@vue/language-core/types/props-fallback.d.ts" />
const store = useStore();
const { hubModelOptions, overview } = storeToRefs(store);
const __VLS_ctx = {
    ...{},
    ...{},
};
let __VLS_components;
let __VLS_intrinsics;
let __VLS_directives;
__VLS_asFunctionalElement1(__VLS_intrinsics.section, __VLS_intrinsics.section)({
    ...{ class: "panel workbench-panel" },
});
/** @type {__VLS_StyleScopedClasses['panel']} */ ;
/** @type {__VLS_StyleScopedClasses['workbench-panel']} */ ;
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
    'data-state': (__VLS_ctx.overview?.hub.running ? 'healthy' : 'idle'),
});
/** @type {__VLS_StyleScopedClasses['status-chip']} */ ;
(__VLS_ctx.overview?.hub.running ? 'hub live' : 'hub booting');
__VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
    ...{ class: "workbench-grid" },
});
/** @type {__VLS_StyleScopedClasses['workbench-grid']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.label, __VLS_intrinsics.label)({
    ...{ class: "field span-field" },
});
/** @type {__VLS_StyleScopedClasses['field']} */ ;
/** @type {__VLS_StyleScopedClasses['span-field']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.span, __VLS_intrinsics.span)({});
__VLS_asFunctionalElement1(__VLS_intrinsics.input)({
    list: "hub-model-options",
    placeholder: "qwen:model-id or chatgpt:model-id",
});
(__VLS_ctx.store.workbenchModel);
__VLS_asFunctionalElement1(__VLS_intrinsics.datalist, __VLS_intrinsics.datalist)({
    id: "hub-model-options",
});
for (const [model] of __VLS_vFor((__VLS_ctx.hubModelOptions))) {
    __VLS_asFunctionalElement1(__VLS_intrinsics.option)({
        key: (model),
        value: (model),
    });
    // @ts-ignore
    [overview, overview, store, hubModelOptions,];
}
__VLS_asFunctionalElement1(__VLS_intrinsics.label, __VLS_intrinsics.label)({
    ...{ class: "field toggle-field" },
});
/** @type {__VLS_StyleScopedClasses['field']} */ ;
/** @type {__VLS_StyleScopedClasses['toggle-field']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.span, __VLS_intrinsics.span)({});
__VLS_asFunctionalElement1(__VLS_intrinsics.input)({
    type: "checkbox",
});
(__VLS_ctx.store.workbenchWebSearch);
__VLS_asFunctionalElement1(__VLS_intrinsics.label, __VLS_intrinsics.label)({
    ...{ class: "field span-field" },
});
/** @type {__VLS_StyleScopedClasses['field']} */ ;
/** @type {__VLS_StyleScopedClasses['span-field']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.span, __VLS_intrinsics.span)({});
__VLS_asFunctionalElement1(__VLS_intrinsics.textarea)({
    value: (__VLS_ctx.store.workbenchPrompt),
    rows: "7",
    placeholder: "Ask for a smoke response and confirm which provider answered.",
});
__VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
    ...{ class: "action-row" },
});
/** @type {__VLS_StyleScopedClasses['action-row']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.button, __VLS_intrinsics.button)({
    ...{ onClick: (...[$event]) => {
            __VLS_ctx.store.runWorkbench();
            // @ts-ignore
            [store, store, store,];
        } },
    ...{ class: "primary-button" },
    disabled: (__VLS_ctx.store.isBusy('workbench:run')),
});
/** @type {__VLS_StyleScopedClasses['primary-button']} */ ;
(__VLS_ctx.store.isBusy('workbench:run') ? 'Running...' : 'Run live probe');
__VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
    ...{ class: "terminal-shell" },
});
/** @type {__VLS_StyleScopedClasses['terminal-shell']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
    ...{ class: "terminal-bar" },
});
/** @type {__VLS_StyleScopedClasses['terminal-bar']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.span, __VLS_intrinsics.span)({});
__VLS_asFunctionalElement1(__VLS_intrinsics.span, __VLS_intrinsics.span)({});
(__VLS_ctx.store.workbenchModel || 'no model selected');
__VLS_asFunctionalElement1(__VLS_intrinsics.pre, __VLS_intrinsics.pre)({
    ...{ class: "code-window large" },
});
/** @type {__VLS_StyleScopedClasses['code-window']} */ ;
/** @type {__VLS_StyleScopedClasses['large']} */ ;
(__VLS_ctx.store.workbenchResponse || 'The live JSON response lands here.');
// @ts-ignore
[store, store, store, store,];
const __VLS_export = (await import('vue')).defineComponent({});
export default {};
