/// <reference types="G:/Tools/RustProxyHub/.toolchain/node_modules/@vue/language-core/types/template-helpers.d.ts" />
/// <reference types="G:/Tools/RustProxyHub/.toolchain/node_modules/@vue/language-core/types/props-fallback.d.ts" />
import { computed } from 'vue';
const props = defineProps();
const store = useStore();
const searchValue = computed({
    get: () => store.searchQuery,
    set: (value) => store.setSearchQuery(value),
});
const hub = computed(() => props.overview?.hub ?? null);
function copyText(value) {
    if (!value)
        return;
    void navigator.clipboard?.writeText(value);
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
__VLS_asFunctionalElement1(__VLS_intrinsics.header, __VLS_intrinsics.header)({
    ...{ class: "hero panel" },
});
/** @type {__VLS_StyleScopedClasses['hero']} */ ;
/** @type {__VLS_StyleScopedClasses['panel']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
    ...{ class: "hero-copy" },
});
/** @type {__VLS_StyleScopedClasses['hero-copy']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.p, __VLS_intrinsics.p)({
    ...{ class: "eyebrow" },
});
/** @type {__VLS_StyleScopedClasses['eyebrow']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.h1, __VLS_intrinsics.h1)({});
__VLS_asFunctionalElement1(__VLS_intrinsics.p, __VLS_intrinsics.p)({
    ...{ class: "lede" },
});
/** @type {__VLS_StyleScopedClasses['lede']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.label, __VLS_intrinsics.label)({
    ...{ class: "field search-field" },
});
/** @type {__VLS_StyleScopedClasses['field']} */ ;
/** @type {__VLS_StyleScopedClasses['search-field']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.span, __VLS_intrinsics.span)({});
__VLS_asFunctionalElement1(__VLS_intrinsics.input)({
    type: "search",
    placeholder: "providers, models, accounts, states, errors",
});
(__VLS_ctx.searchValue);
__VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
    ...{ class: "hero-rail" },
});
/** @type {__VLS_StyleScopedClasses['hero-rail']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
    ...{ class: "hero-warning" },
});
/** @type {__VLS_StyleScopedClasses['hero-warning']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.span, __VLS_intrinsics.span)({
    ...{ class: "warning-mark" },
});
/** @type {__VLS_StyleScopedClasses['warning-mark']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.p, __VLS_intrinsics.p)({});
__VLS_asFunctionalElement1(__VLS_intrinsics.code, __VLS_intrinsics.code)({});
__VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
    ...{ class: "hero-actions" },
});
/** @type {__VLS_StyleScopedClasses['hero-actions']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.button, __VLS_intrinsics.button)({
    ...{ onClick: (...[$event]) => {
            __VLS_ctx.store.refreshOverview();
            // @ts-ignore
            [searchValue, store,];
        } },
    ...{ class: "primary-button" },
    disabled: (__VLS_ctx.store.isRefreshing),
});
/** @type {__VLS_StyleScopedClasses['primary-button']} */ ;
(__VLS_ctx.store.isRefreshing ? 'Refreshing...' : 'Refresh surveillance');
__VLS_asFunctionalElement1(__VLS_intrinsics.button, __VLS_intrinsics.button)({
    ...{ onClick: (...[$event]) => {
            __VLS_ctx.copyText(__VLS_ctx.hub?.base_url);
            // @ts-ignore
            [store, store, copyText, hub,];
        } },
    ...{ class: "ghost-button" },
    disabled: (!__VLS_ctx.hub?.base_url),
});
/** @type {__VLS_StyleScopedClasses['ghost-button']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.button, __VLS_intrinsics.button)({
    ...{ onClick: (...[$event]) => {
            __VLS_ctx.copyText(__VLS_ctx.hub?.openapi_url);
            // @ts-ignore
            [copyText, hub, hub,];
        } },
    ...{ class: "ghost-button" },
    disabled: (!__VLS_ctx.hub?.openapi_url),
});
/** @type {__VLS_StyleScopedClasses['ghost-button']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
    ...{ class: "stat-stack" },
});
/** @type {__VLS_StyleScopedClasses['stat-stack']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
    ...{ class: "stat-pill" },
});
/** @type {__VLS_StyleScopedClasses['stat-pill']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.span, __VLS_intrinsics.span)({});
__VLS_asFunctionalElement1(__VLS_intrinsics.strong, __VLS_intrinsics.strong)({});
(__VLS_ctx.hub?.health_status ?? 'booting');
__VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
    ...{ class: "stat-pill" },
});
/** @type {__VLS_StyleScopedClasses['stat-pill']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.span, __VLS_intrinsics.span)({});
__VLS_asFunctionalElement1(__VLS_intrinsics.strong, __VLS_intrinsics.strong)({});
(__VLS_ctx.hub?.model_count ?? 0);
__VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
    ...{ class: "stat-pill" },
});
/** @type {__VLS_StyleScopedClasses['stat-pill']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.span, __VLS_intrinsics.span)({});
__VLS_asFunctionalElement1(__VLS_intrinsics.strong, __VLS_intrinsics.strong)({});
(__VLS_ctx.overview?.qwen_account_count ?? 0);
__VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
    ...{ class: "stat-pill" },
});
/** @type {__VLS_StyleScopedClasses['stat-pill']} */ ;
__VLS_asFunctionalElement1(__VLS_intrinsics.span, __VLS_intrinsics.span)({});
__VLS_asFunctionalElement1(__VLS_intrinsics.strong, __VLS_intrinsics.strong)({});
(__VLS_ctx.store.openLoginCount);
__VLS_asFunctionalElement1(__VLS_intrinsics.div, __VLS_intrinsics.div)({
    ...{ class: "hub-ribbon" },
});
/** @type {__VLS_StyleScopedClasses['hub-ribbon']} */ ;
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
(__VLS_ctx.hub?.base_url ?? 'booting embedded hub...');
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
(__VLS_ctx.hub?.openapi_url ?? 'waiting for /openapi.json');
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
(__VLS_ctx.overview?.helper_dir ?? 'resolving helper assets...');
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
(__VLS_ctx.overview?.app_data_dir ?? 'resolving app data dir...');
// @ts-ignore
[store, hub, hub, hub, hub, hub, overview, overview, overview,];
const __VLS_export = (await import('vue')).defineComponent({
    __typeProps: {},
});
export default {};
