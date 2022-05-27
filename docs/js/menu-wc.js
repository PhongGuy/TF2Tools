'use strict';

customElements.define('compodoc-menu', class extends HTMLElement {
    constructor() {
        super();
        this.isNormalMode = this.getAttribute('mode') === 'normal';
    }

    connectedCallback() {
        this.render(this.isNormalMode);
    }

    render(isNormalMode) {
        let tp = lithtml.html(`
        <nav>
            <ul class="list">
                <li class="title">
                    <a href="index.html" data-type="index-link">TF2Tools</a>
                </li>

                <li class="divider"></li>
                ${ isNormalMode ? `<div id="book-search-input" role="search"><input type="text" placeholder="Type to search"></div>` : '' }
                <li class="chapter">
                    <a data-type="chapter-link" href="index.html"><span class="icon ion-ios-home"></span>Getting started</a>
                    <ul class="links">
                        <li class="link">
                            <a href="overview.html" data-type="chapter-link">
                                <span class="icon ion-ios-keypad"></span>Overview
                            </a>
                        </li>
                        <li class="link">
                            <a href="index.html" data-type="chapter-link">
                                <span class="icon ion-ios-paper"></span>README
                            </a>
                        </li>
                        <li class="link">
                            <a href="changelog.html"  data-type="chapter-link">
                                <span class="icon ion-ios-paper"></span>CHANGELOG
                            </a>
                        </li>
                        <li class="link">
                            <a href="license.html"  data-type="chapter-link">
                                <span class="icon ion-ios-paper"></span>LICENSE
                            </a>
                        </li>
                                <li class="link">
                                    <a href="dependencies.html" data-type="chapter-link">
                                        <span class="icon ion-ios-list"></span>Dependencies
                                    </a>
                                </li>
                                <li class="link">
                                    <a href="properties.html" data-type="chapter-link">
                                        <span class="icon ion-ios-apps"></span>Properties
                                    </a>
                                </li>
                    </ul>
                </li>
                    <li class="chapter modules">
                        <a data-type="chapter-link" href="modules.html">
                            <div class="menu-toggler linked" data-toggle="collapse" ${ isNormalMode ?
                                'data-target="#modules-links"' : 'data-target="#xs-modules-links"' }>
                                <span class="icon ion-ios-archive"></span>
                                <span class="link-name">Modules</span>
                                <span class="icon ion-ios-arrow-down"></span>
                            </div>
                        </a>
                        <ul class="links collapse " ${ isNormalMode ? 'id="modules-links"' : 'id="xs-modules-links"' }>
                            <li class="link">
                                <a href="modules/AppModule.html" data-type="entity-link" >AppModule</a>
                                    <li class="chapter inner">
                                        <div class="simple menu-toggler" data-toggle="collapse" ${ isNormalMode ?
                                            'data-target="#components-links-module-AppModule-de55b3985f2182668cf81cb389fac089a969a3fbb40852e4cd634d6fc72d03a4e45438324eff2631e6923dfc82e8ed98be3221a6bcc9720dbc6faa237e397d30"' : 'data-target="#xs-components-links-module-AppModule-de55b3985f2182668cf81cb389fac089a969a3fbb40852e4cd634d6fc72d03a4e45438324eff2631e6923dfc82e8ed98be3221a6bcc9720dbc6faa237e397d30"' }>
                                            <span class="icon ion-md-cog"></span>
                                            <span>Components</span>
                                            <span class="icon ion-ios-arrow-down"></span>
                                        </div>
                                        <ul class="links collapse" ${ isNormalMode ? 'id="components-links-module-AppModule-de55b3985f2182668cf81cb389fac089a969a3fbb40852e4cd634d6fc72d03a4e45438324eff2631e6923dfc82e8ed98be3221a6bcc9720dbc6faa237e397d30"' :
                                            'id="xs-components-links-module-AppModule-de55b3985f2182668cf81cb389fac089a969a3fbb40852e4cd634d6fc72d03a4e45438324eff2631e6923dfc82e8ed98be3221a6bcc9720dbc6faa237e397d30"' }>
                                            <li class="link">
                                                <a href="components/AppComponent.html" data-type="entity-link" data-context="sub-entity" data-context-id="modules" >AppComponent</a>
                                            </li>
                                            <li class="link">
                                                <a href="components/CrosshairsComponent.html" data-type="entity-link" data-context="sub-entity" data-context-id="modules" >CrosshairsComponent</a>
                                            </li>
                                            <li class="link">
                                                <a href="components/DashboardComponent.html" data-type="entity-link" data-context="sub-entity" data-context-id="modules" >DashboardComponent</a>
                                            </li>
                                            <li class="link">
                                                <a href="components/HitsoundComponent.html" data-type="entity-link" data-context="sub-entity" data-context-id="modules" >HitsoundComponent</a>
                                            </li>
                                            <li class="link">
                                                <a href="components/HudComponent.html" data-type="entity-link" data-context="sub-entity" data-context-id="modules" >HudComponent</a>
                                            </li>
                                            <li class="link">
                                                <a href="components/MultipleWarningComponent.html" data-type="entity-link" data-context="sub-entity" data-context-id="modules" >MultipleWarningComponent</a>
                                            </li>
                                            <li class="link">
                                                <a href="components/QuestionAnswerComponent.html" data-type="entity-link" data-context="sub-entity" data-context-id="modules" >QuestionAnswerComponent</a>
                                            </li>
                                            <li class="link">
                                                <a href="components/SettingsComponent.html" data-type="entity-link" data-context="sub-entity" data-context-id="modules" >SettingsComponent</a>
                                            </li>
                                            <li class="link">
                                                <a href="components/SetupComponent.html" data-type="entity-link" data-context="sub-entity" data-context-id="modules" >SetupComponent</a>
                                            </li>
                                            <li class="link">
                                                <a href="components/UploadChangeNameComponent.html" data-type="entity-link" data-context="sub-entity" data-context-id="modules" >UploadChangeNameComponent</a>
                                            </li>
                                            <li class="link">
                                                <a href="components/WeaponsSoundsComponent.html" data-type="entity-link" data-context="sub-entity" data-context-id="modules" >WeaponsSoundsComponent</a>
                                            </li>
                                            <li class="link">
                                                <a href="components/YesNoComponent.html" data-type="entity-link" data-context="sub-entity" data-context-id="modules" >YesNoComponent</a>
                                            </li>
                                        </ul>
                                    </li>
                            </li>
                            <li class="link">
                                <a href="modules/AppRoutingModule.html" data-type="entity-link" >AppRoutingModule</a>
                            </li>
                            <li class="link">
                                <a href="modules/CoreModule.html" data-type="entity-link" >CoreModule</a>
                            </li>
                            <li class="link">
                                <a href="modules/MaterialModule.html" data-type="entity-link" >MaterialModule</a>
                            </li>
                </ul>
                </li>
                    <li class="chapter">
                        <div class="simple menu-toggler" data-toggle="collapse" ${ isNormalMode ? 'data-target="#classes-links"' :
                            'data-target="#xs-classes-links"' }>
                            <span class="icon ion-ios-paper"></span>
                            <span>Classes</span>
                            <span class="icon ion-ios-arrow-down"></span>
                        </div>
                        <ul class="links collapse " ${ isNormalMode ? 'id="classes-links"' : 'id="xs-classes-links"' }>
                            <li class="link">
                                <a href="classes/CrosshairSelected.html" data-type="entity-link" >CrosshairSelected</a>
                            </li>
                            <li class="link">
                                <a href="classes/Hitsound.html" data-type="entity-link" >Hitsound</a>
                            </li>
                            <li class="link">
                                <a href="classes/Hud.html" data-type="entity-link" >Hud</a>
                            </li>
                            <li class="link">
                                <a href="classes/HudList.html" data-type="entity-link" >HudList</a>
                            </li>
                            <li class="link">
                                <a href="classes/LatestRelease.html" data-type="entity-link" >LatestRelease</a>
                            </li>
                            <li class="link">
                                <a href="classes/QuestionAnswer.html" data-type="entity-link" >QuestionAnswer</a>
                            </li>
                            <li class="link">
                                <a href="classes/Settings.html" data-type="entity-link" >Settings</a>
                            </li>
                            <li class="link">
                                <a href="classes/SnackBarMessage.html" data-type="entity-link" >SnackBarMessage</a>
                            </li>
                            <li class="link">
                                <a href="classes/TfWeapons.html" data-type="entity-link" >TfWeapons</a>
                            </li>
                            <li class="link">
                                <a href="classes/Update.html" data-type="entity-link" >Update</a>
                            </li>
                            <li class="link">
                                <a href="classes/UploadChangeName.html" data-type="entity-link" >UploadChangeName</a>
                            </li>
                            <li class="link">
                                <a href="classes/VtfCrosshair.html" data-type="entity-link" >VtfCrosshair</a>
                            </li>
                            <li class="link">
                                <a href="classes/WeaponData.html" data-type="entity-link" >WeaponData</a>
                            </li>
                            <li class="link">
                                <a href="classes/YesNo.html" data-type="entity-link" >YesNo</a>
                            </li>
                        </ul>
                    </li>
                        <li class="chapter">
                            <div class="simple menu-toggler" data-toggle="collapse" ${ isNormalMode ? 'data-target="#injectables-links"' :
                                'data-target="#xs-injectables-links"' }>
                                <span class="icon ion-md-arrow-round-down"></span>
                                <span>Injectables</span>
                                <span class="icon ion-ios-arrow-down"></span>
                            </div>
                            <ul class="links collapse " ${ isNormalMode ? 'id="injectables-links"' : 'id="xs-injectables-links"' }>
                                <li class="link">
                                    <a href="injectables/ElectronService.html" data-type="entity-link" >ElectronService</a>
                                </li>
                                <li class="link">
                                    <a href="injectables/FileHelpService.html" data-type="entity-link" >FileHelpService</a>
                                </li>
                                <li class="link">
                                    <a href="injectables/LogService.html" data-type="entity-link" >LogService</a>
                                </li>
                                <li class="link">
                                    <a href="injectables/SnackService.html" data-type="entity-link" >SnackService</a>
                                </li>
                                <li class="link">
                                    <a href="injectables/ValidatorService.html" data-type="entity-link" >ValidatorService</a>
                                </li>
                            </ul>
                        </li>
                    <li class="chapter">
                        <div class="simple menu-toggler" data-toggle="collapse" ${ isNormalMode ? 'data-target="#interfaces-links"' :
                            'data-target="#xs-interfaces-links"' }>
                            <span class="icon ion-md-information-circle-outline"></span>
                            <span>Interfaces</span>
                            <span class="icon ion-ios-arrow-down"></span>
                        </div>
                        <ul class="links collapse " ${ isNormalMode ? ' id="interfaces-links"' : 'id="xs-interfaces-links"' }>
                            <li class="link">
                                <a href="interfaces/Asset.html" data-type="entity-link" >Asset</a>
                            </li>
                            <li class="link">
                                <a href="interfaces/Author.html" data-type="entity-link" >Author</a>
                            </li>
                            <li class="link">
                                <a href="interfaces/LogActions.html" data-type="entity-link" >LogActions</a>
                            </li>
                        </ul>
                    </li>
                    <li class="chapter">
                        <div class="simple menu-toggler" data-toggle="collapse" ${ isNormalMode ? 'data-target="#miscellaneous-links"'
                            : 'data-target="#xs-miscellaneous-links"' }>
                            <span class="icon ion-ios-cube"></span>
                            <span>Miscellaneous</span>
                            <span class="icon ion-ios-arrow-down"></span>
                        </div>
                        <ul class="links collapse " ${ isNormalMode ? 'id="miscellaneous-links"' : 'id="xs-miscellaneous-links"' }>
                            <li class="link">
                                <a href="miscellaneous/variables.html" data-type="entity-link">Variables</a>
                            </li>
                        </ul>
                    </li>
                        <li class="chapter">
                            <a data-type="chapter-link" href="routes.html"><span class="icon ion-ios-git-branch"></span>Routes</a>
                        </li>
                    <li class="chapter">
                        <a data-type="chapter-link" href="coverage.html"><span class="icon ion-ios-stats"></span>Documentation coverage</a>
                    </li>
                    <li class="divider"></li>
                    <li class="copyright">
                        Documentation generated using <a href="https://compodoc.app/" target="_blank">
                            <img data-src="images/compodoc-vectorise.png" class="img-responsive" data-type="compodoc-logo">
                        </a>
                    </li>
            </ul>
        </nav>
        `);
        this.innerHTML = tp.strings;
    }
});