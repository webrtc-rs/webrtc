use media_engine::*;
use setting_engine::*;

pub mod media_engine;
pub mod setting_engine;

/// API bundles the global functions of the WebRTC and ORTC API.
/// Some of these functions are also exported globally using the
/// defaultAPI object. Note that the global version of the API
/// may be phased out in the future.
pub struct Api {
    setting_engine: SettingEngine,
    media_engine: MediaEngine,
    //TODO: interceptor   interceptor.Interceptor
}

pub struct ApiBuilder {
    api: Api,
}

impl Default for ApiBuilder {
    fn default() -> Self {
        ApiBuilder {
            api: Api {
                setting_engine: SettingEngine::default(),
                media_engine: MediaEngine::default(),
            },
        }
    }
}

impl ApiBuilder {
    pub fn new() -> Self {
        ApiBuilder::default()
    }

    pub fn build(self) -> Api {
        self.api
    }

    /// WithSettingEngine allows providing a SettingEngine to the API.
    /// Settings should not be changed after passing the engine to an API.
    pub fn with_setting_engine(mut self, setting_engine: SettingEngine) -> Self {
        self.api.setting_engine = setting_engine;
        self
    }

    /// WithMediaEngine allows providing a MediaEngine to the API.
    /// Settings can be changed after passing the engine to an API.
    pub fn with_media_engine(mut self, media_engine: MediaEngine) -> Self {
        self.api.media_engine = media_engine;
        self
    }

    //TODO:
    // WithInterceptorRegistry allows providing Interceptors to the API.
    // Settings should not be changed after passing the registry to an API.
    /*pub WithInterceptorRegistry(interceptorRegistry *interceptor.Registry) func(a *API) {
        return func(a *API) {
            a.interceptor = interceptorRegistry.Build()
        }
    }*/

    /*TODO:
    // NewICEGatherer creates a new NewICEGatherer.
    // This constructor is part of the ORTC API. It is not
    // meant to be used together with the basic WebRTC API.
    func (api *API) NewICEGatherer(opts ICEGatherOptions) (*ICEGatherer, error) {
        var validatedServers []*ice.URL
        if len(opts.ICEServers) > 0 {
            for _, server := range opts.ICEServers {
                url, err := server.urls()
                if err != nil {
                    return nil, err
                }
                validatedServers = append(validatedServers, url...)
            }
        }

        return &ICEGatherer{
            state:            ICEGathererStateNew,
            gatherPolicy:     opts.ICEGatherPolicy,
            validatedServers: validatedServers,
            api:              api,
            log:              api.settingEngine.LoggerFactory.NewLogger("ice"),
        }, nil
    }*/
}
