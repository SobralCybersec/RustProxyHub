fn openapi_document(config: &AppConfig) -> Value {
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "RustProxyHub Unified API",
            "version": "0.1.0",
            "description": "Unified OpenAI-compatible gateway for the embedded Qwen, DeepSeek, Kimi, ChatGPT, Gemini, Mistral, Z.AI, and Meta AI proxy services."
        },
        "servers": [
            { "url": format!("http://127.0.0.1:{}", config.port) }
        ],
        "components": {
            "securitySchemes": {
                "BearerAuth": {
                    "type": "http",
                    "scheme": "bearer"
                }
            },
            "schemas": {
                "ChatMessage": {
                    "type": "object",
                    "required": ["role"],
                    "properties": {
                        "role": { "type": "string" },
                        "content": {
                            "oneOf": [
                                { "type": "string" },
                                { "type": "array" },
                                { "type": "object" },
                                { "type": "null" }
                            ]
                        },
                        "tool_calls": { "type": "array" },
                        "tool_call_id": { "type": "string" },
                        "name": { "type": "string" },
                        "reasoning_content": { "type": "string" }
                    }
                },
                "ChatCompletionRequest": {
                    "type": "object",
                    "required": ["model", "messages"],
                    "properties": {
                        "model": {
                            "type": "string",
                            "description": "Model id. Raw ids are auto-routed; optional provider prefixes like qwen:model-id are also accepted."
                        },
                        "messages": {
                            "type": "array",
                            "items": { "$ref": "#/components/schemas/ChatMessage" }
                        },
                        "stream": { "type": "boolean" },
                        "tools": { "type": "array" },
                        "tool_choice": {},
                        "stream_options": {
                            "type": "object",
                            "properties": {
                                "include_usage": { "type": "boolean" }
                            }
                        }
                    }
                },
                "StopRequest": {
                    "type": "object",
                    "properties": {
                        "completion_id": { "type": "string" },
                        "chat_id": { "type": "string" },
                        "response_id": { "type": "string" }
                    }
                }
            }
        },
        "paths": {
            "/health": {
                "get": {
                    "summary": "Hub health and provider reachability",
                    "responses": {
                        "200": {
                            "description": "Hub health payload"
                        }
                    }
                }
            },
            "/providers": {
                "get": {
                    "summary": "List upstream provider status snapshots",
                    "responses": {
                        "200": { "description": "Provider status list" }
                    }
                }
            },
            "/providers/{provider}/logs": {
                "get": {
                    "summary": "Read recent provider bridge log entries",
                    "security": [{ "BearerAuth": [] }],
                    "parameters": [{
                        "name": "provider",
                        "in": "path",
                        "required": true,
                        "schema": { "type": "string" }
                    }],
                    "responses": {
                        "200": { "description": "Provider log entries" },
                        "401": { "description": "Unauthorized" },
                        "404": { "description": "Unknown provider" }
                    }
                }
            },
            "/openapi.json": {
                "get": {
                    "summary": "OpenAPI specification for the unified hub",
                    "responses": {
                        "200": { "description": "OpenAPI document" }
                    }
                }
            },
            "/v1/models": {
                "get": {
                    "summary": "Merged model list across Qwen, DeepSeek, and Kimi",
                    "security": [{ "BearerAuth": [] }],
                    "responses": {
                        "200": { "description": "OpenAI-style model list" },
                        "401": { "description": "Unauthorized" }
                    }
                }
            },
            "/v1/models/{model}": {
                "get": {
                    "summary": "Look up one merged model by id",
                    "security": [{ "BearerAuth": [] }],
                    "parameters": [
                        {
                            "name": "model",
                            "in": "path",
                            "required": true,
                            "schema": { "type": "string" }
                        }
                    ],
                    "responses": {
                        "200": { "description": "Model payload" },
                        "404": { "description": "Model not found" }
                    }
                }
            },
            "/v1/chat/completions": {
                "post": {
                    "summary": "Route one OpenAI chat request to the matching upstream provider",
                    "security": [{ "BearerAuth": [] }],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/ChatCompletionRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Chat completion or SSE stream" },
                        "401": { "description": "Unauthorized" },
                        "502": { "description": "Upstream proxy error" }
                    }
                }
            },
            "/v1/responses": {
                "post": {
                    "summary": "Route one OpenAI Responses request to the matching upstream provider",
                    "security": [{ "BearerAuth": [] }],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "type": "object" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Responses API payload or SSE stream" },
                        "401": { "description": "Unauthorized" },
                        "502": { "description": "Upstream proxy error" }
                    }
                }
            },
            "/v1/messages": {
                "post": {
                    "summary": "Route one Anthropic Messages request to the matching upstream provider",
                    "security": [{ "BearerAuth": [] }],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "type": "object" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Anthropic message payload or SSE stream" },
                        "401": { "description": "Unauthorized" },
                        "502": { "description": "Upstream proxy error" }
                    }
                }
            },
            "/v1/messages/count_tokens": {
                "post": {
                    "summary": "Route one Anthropic token-count request to the matching upstream provider",
                    "security": [{ "BearerAuth": [] }],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "type": "object" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Anthropic token count payload" },
                        "401": { "description": "Unauthorized" },
                        "502": { "description": "Upstream proxy error" }
                    }
                }
            },
            "/v1/chat/completions/stop": {
                "post": {
                    "summary": "Forward a stop request to the Qwen proxy",
                    "security": [{ "BearerAuth": [] }],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/StopRequest" }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Stop response" }
                    }
                }
            },
            "/v1/upload": {
                "post": {
                    "summary": "Forward multipart uploads to the Qwen proxy",
                    "security": [{ "BearerAuth": [] }],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "multipart/form-data": {
                                "schema": {
                                    "type": "object",
                                    "properties": {
                                        "file": {
                                            "type": "string",
                                            "format": "binary"
                                        }
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Upload response" }
                    }
                }
            }
        }
    })
}

