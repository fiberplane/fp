# \DefaultApi

All URIs are relative to *https://fiberplane.com*

Method | HTTP request | Description
------------- | ------------- | -------------
[**delete_file**](DefaultApi.md#delete_file) | **DELETE** /api/files/{notebookId}/{fileId} | 
[**delete_notebook**](DefaultApi.md#delete_notebook) | **DELETE** /api/notebooks/{id} | 
[**file_upload**](DefaultApi.md#file_upload) | **POST** /api/files/{notebookId} | 
[**get_file**](DefaultApi.md#get_file) | **GET** /api/files/{notebookId}/{fileId} | 
[**get_notebook**](DefaultApi.md#get_notebook) | **GET** /api/notebooks/{id} | 
[**get_profile**](DefaultApi.md#get_profile) | **GET** /api/profile | 
[**get_profile_picture**](DefaultApi.md#get_profile_picture) | **GET** /api/profile/picture | 
[**logout**](DefaultApi.md#logout) | **POST** /api/logout | 
[**notebook_create**](DefaultApi.md#notebook_create) | **POST** /api/notebooks | 
[**notebook_list**](DefaultApi.md#notebook_list) | **GET** /api/notebooks | 
[**oidc_authorize_google**](DefaultApi.md#oidc_authorize_google) | **GET** /api/oidc/authorize/google | 
[**org_data_source_create**](DefaultApi.md#org_data_source_create) | **POST** /api/datasources | 
[**patch_notebook**](DefaultApi.md#patch_notebook) | **PATCH** /api/notebooks/{id} | 
[**pinned_notebook_create**](DefaultApi.md#pinned_notebook_create) | **POST** /api/pinnednotebooks | 
[**pinned_notebook_delete**](DefaultApi.md#pinned_notebook_delete) | **DELETE** /api/pinnednotebooks/{notebookId} | 
[**pinned_notebook_list**](DefaultApi.md#pinned_notebook_list) | **GET** /api/pinnednotebooks | 
[**proxy_create**](DefaultApi.md#proxy_create) | **POST** /api/proxies | 
[**proxy_data_sources_list**](DefaultApi.md#proxy_data_sources_list) | **GET** /api/proxies/datasources | 
[**proxy_delete**](DefaultApi.md#proxy_delete) | **DELETE** /api/proxies/{proxyId} | 
[**proxy_get**](DefaultApi.md#proxy_get) | **GET** /api/proxies/{proxyId} | 
[**proxy_list**](DefaultApi.md#proxy_list) | **GET** /api/proxies | 
[**proxy_relay**](DefaultApi.md#proxy_relay) | **POST** /api/proxies/{proxyId}/relay | 
[**trigger_create**](DefaultApi.md#trigger_create) | **POST** /api/triggers | 
[**trigger_delete**](DefaultApi.md#trigger_delete) | **DELETE** /api/triggers/{triggerId} | 
[**trigger_get**](DefaultApi.md#trigger_get) | **GET** /api/triggers/{triggerId} | 
[**trigger_invoke**](DefaultApi.md#trigger_invoke) | **POST** /api/triggers/{triggerId}/webhook | 
[**trigger_list**](DefaultApi.md#trigger_list) | **GET** /api/triggers | 
[**update_profile_picture**](DefaultApi.md#update_profile_picture) | **POST** /api/profile/picture | 



## delete_file

> delete_file(notebook_id, file_id)


Delete a file

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**notebook_id** | **String** | ID of the notebook | [required] |
**file_id** | **String** | ID of the file | [required] |

### Return type

 (empty response body)

### Authorization

[userToken](../README.md#userToken)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: Not defined

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## delete_notebook

> delete_notebook(id)


### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**id** | **String** | ID of the notebook | [required] |

### Return type

 (empty response body)

### Authorization

[userToken](../README.md#userToken)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: Not defined

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## file_upload

> crate::models::FileSummary file_upload(notebook_id, file)


upload a file

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**notebook_id** | **String** | ID of the notebook | [required] |
**file** | **std::path::PathBuf** |  | [required] |

### Return type

[**crate::models::FileSummary**](fileSummary.md)

### Authorization

[userToken](../README.md#userToken)

### HTTP request headers

- **Content-Type**: multipart/form-data
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_file

> std::path::PathBuf get_file(notebook_id, file_id)


Get a file

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**notebook_id** | **String** | ID of the notebook | [required] |
**file_id** | **String** | ID of the file | [required] |

### Return type

[**std::path::PathBuf**](std::path::PathBuf.md)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: image/_*

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_notebook

> crate::models::Notebook get_notebook(id)


Fetch a single notebook

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**id** | **String** | ID of the notebook | [required] |

### Return type

[**crate::models::Notebook**](notebook.md)

### Authorization

[userToken](../README.md#userToken)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_profile

> crate::models::User get_profile()


Fetch the profile of the authenticated user

### Parameters

This endpoint does not need any parameter.

### Return type

[**crate::models::User**](user.md)

### Authorization

[userToken](../README.md#userToken)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_profile_picture

> std::path::PathBuf get_profile_picture()


Retrieve profile image

### Parameters

This endpoint does not need any parameter.

### Return type

[**std::path::PathBuf**](std::path::PathBuf.md)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: image/_*

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## logout

> logout()


Log out of Fiberplane

### Parameters

This endpoint does not need any parameter.

### Return type

 (empty response body)

### Authorization

[userToken](../README.md#userToken)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: Not defined

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## notebook_create

> crate::models::Notebook notebook_create(new_notebook)


Create a new notebook

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**new_notebook** | Option<[**NewNotebook**](NewNotebook.md)> | new notebook |  |

### Return type

[**crate::models::Notebook**](notebook.md)

### Authorization

[userToken](../README.md#userToken)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## notebook_list

> Vec<crate::models::NotebookSummary> notebook_list()


List all accessible notebooks

### Parameters

This endpoint does not need any parameter.

### Return type

[**Vec<crate::models::NotebookSummary>**](notebookSummary.md)

### Authorization

[userToken](../README.md#userToken)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## oidc_authorize_google

> oidc_authorize_google(cli_redirect_port, redirect)


Start the Google OAuth flow to authenticate a user

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**cli_redirect_port** | Option<**i32**> | The port on localhost to redirect to after the OAuth flow is successful. Used for authorizing the CLI |  |
**redirect** | Option<**String**> | Relative path to redirect to after the OAuth flow is successful. Used for deep linking into the Studio |  |

### Return type

 (empty response body)

### Authorization

[userToken](../README.md#userToken)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: Not defined

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## org_data_source_create

> crate::models::OrgDataSource org_data_source_create(new_org_data_source)


Create an organization data-source

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**new_org_data_source** | Option<[**NewOrgDataSource**](NewOrgDataSource.md)> | new data-source |  |

### Return type

[**crate::models::OrgDataSource**](orgDataSource.md)

### Authorization

[userToken](../README.md#userToken)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## patch_notebook

> patch_notebook(id, notebook_patch)


Modifies individual properties of a single notebook

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**id** | **String** | ID of the notebook | [required] |
**notebook_patch** | Option<[**NotebookPatch**](NotebookPatch.md)> | updated properties |  |

### Return type

 (empty response body)

### Authorization

[userToken](../README.md#userToken)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: Not defined

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## pinned_notebook_create

> pinned_notebook_create(new_pinned_notebook)


Create a new notebook

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**new_pinned_notebook** | Option<[**NewPinnedNotebook**](NewPinnedNotebook.md)> | new notebook |  |

### Return type

 (empty response body)

### Authorization

[userToken](../README.md#userToken)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: Not defined

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## pinned_notebook_delete

> pinned_notebook_delete(notebook_id)


### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**notebook_id** | **String** | ID of the notebook | [required] |

### Return type

 (empty response body)

### Authorization

[userToken](../README.md#userToken)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: Not defined

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## pinned_notebook_list

> Vec<crate::models::NotebookSummary> pinned_notebook_list()


List all pinned notebooks

### Parameters

This endpoint does not need any parameter.

### Return type

[**Vec<crate::models::NotebookSummary>**](notebookSummary.md)

### Authorization

[userToken](../README.md#userToken)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## proxy_create

> crate::models::Proxy proxy_create(new_proxy)


Create a new proxy

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**new_proxy** | Option<[**NewProxy**](NewProxy.md)> | new proxy |  |

### Return type

[**crate::models::Proxy**](proxy.md)

### Authorization

[userToken](../README.md#userToken)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## proxy_data_sources_list

> Vec<crate::models::DataSourceAndProxySummary> proxy_data_sources_list()


Get all of the data sources for all proxies that belong to the same organization as the user

### Parameters

This endpoint does not need any parameter.

### Return type

[**Vec<crate::models::DataSourceAndProxySummary>**](dataSourceAndProxySummary.md)

### Authorization

[userToken](../README.md#userToken)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## proxy_delete

> proxy_delete(proxy_id)


### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**proxy_id** | **String** | ID of the proxy | [required] |

### Return type

 (empty response body)

### Authorization

[userToken](../README.md#userToken)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: Not defined

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## proxy_get

> crate::models::Proxy proxy_get(proxy_id)


Retrieve a single proxy

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**proxy_id** | **String** | ID of the proxy | [required] |

### Return type

[**crate::models::Proxy**](proxy.md)

### Authorization

[userToken](../README.md#userToken)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## proxy_list

> Vec<crate::models::ProxySummary> proxy_list()


List all proxies

### Parameters

This endpoint does not need any parameter.

### Return type

[**Vec<crate::models::ProxySummary>**](proxySummary.md)

### Authorization

[userToken](../README.md#userToken)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## proxy_relay

> proxy_relay(proxy_id, data_source_name)


Relay a query to a remote proxy

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**proxy_id** | **String** | ID of the proxy | [required] |
**data_source_name** | **String** | Name of the data source | [required] |

### Return type

 (empty response body)

### Authorization

[userToken](../README.md#userToken)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: Not defined

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## trigger_create

> crate::models::Trigger trigger_create(new_trigger)


Create a new trigger

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**new_trigger** | Option<[**NewTrigger**](NewTrigger.md)> | Template URL or body |  |

### Return type

[**crate::models::Trigger**](trigger.md)

### Authorization

[userToken](../README.md#userToken)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## trigger_delete

> trigger_delete(trigger_id)


### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**trigger_id** | **String** | ID of the trigger | [required] |

### Return type

 (empty response body)

### Authorization

[userToken](../README.md#userToken)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: Not defined

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## trigger_get

> crate::models::Trigger trigger_get(trigger_id)


### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**trigger_id** | **String** | ID of the trigger | [required] |

### Return type

[**crate::models::Trigger**](trigger.md)

### Authorization

[userToken](../README.md#userToken)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## trigger_invoke

> crate::models::TriggerWebHookResponse trigger_invoke(trigger_id, body)


Invoke a trigger to create a notebook from the associated template

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**trigger_id** | **String** | ID of the trigger | [required] |
**body** | Option<**serde_json::Value**> | Parameters to pass to the template |  |

### Return type

[**crate::models::TriggerWebHookResponse**](triggerWebHookResponse.md)

### Authorization

[userToken](../README.md#userToken)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## trigger_list

> Vec<crate::models::Trigger> trigger_list()


### Parameters

This endpoint does not need any parameter.

### Return type

[**Vec<crate::models::Trigger>**](trigger.md)

### Authorization

[userToken](../README.md#userToken)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## update_profile_picture

> update_profile_picture(picture)


Upload profile image

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**picture** | **std::path::PathBuf** |  | [required] |

### Return type

 (empty response body)

### Authorization

[userToken](../README.md#userToken)

### HTTP request headers

- **Content-Type**: multipart/form-data
- **Accept**: Not defined

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)

