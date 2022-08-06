import {HttpClientModule} from '@angular/common/http';
import {NgModule} from '@angular/core';
import {FormsModule} from '@angular/forms';
import {MAT_DIALOG_DEFAULT_OPTIONS} from '@angular/material/dialog';
import {MAT_SELECT_CONFIG} from '@angular/material/select';
import {BrowserModule} from '@angular/platform-browser';
import {BrowserAnimationsModule} from '@angular/platform-browser/animations';
import {AppRoutingModule} from './app-routing.module';
import {AppComponent} from './app.component';
import {CoreModule} from './core/core.module';
import {CrosshairsComponent} from './dashboard/crosshairs/crosshairs.component';
import {DashboardComponent} from './dashboard/dashboard.component';
import {HitsoundComponent} from './dashboard/hitsound/hitsound.component';
import {MultipleWarningComponent} from './dashboard/hitsound/multiple-warning/multiple-warning.component';
import {UploadChangeNameComponent} from './dashboard/hitsound/upload-change-name/upload-change-name.component';
import {HudComponent} from './dashboard/hud/hud.component';
import {SettingsComponent} from './dashboard/settings/settings.component';
import {WeaponsSoundsComponent} from './dashboard/weapons-sounds/weapons-sounds.component';
import {QuestionAnswerComponent} from './dialogs/question-answer/question-answer.component';
import {YesNoComponent} from './dialogs/yes-no/yes-no.component';
import {MaterialModule} from './material';
import {SetupComponent} from './setup/setup.component';
import {A11yModule} from "@angular/cdk/a11y";

/**
 * App Module
 */
@NgModule({
  declarations: [
    AppComponent,
    DashboardComponent,
    HudComponent,
    HitsoundComponent,
    QuestionAnswerComponent,
    YesNoComponent,
    CrosshairsComponent,
    WeaponsSoundsComponent,
    MultipleWarningComponent,
    SetupComponent,
    UploadChangeNameComponent,
    SettingsComponent
  ],
  imports: [
    BrowserModule,
    BrowserAnimationsModule,
    FormsModule,
    HttpClientModule,
    CoreModule,
    MaterialModule,
    AppRoutingModule,
    A11yModule
  ],
  providers: [
    {provide: MAT_SELECT_CONFIG, useValue: {disableOptionCentering: true}},
    {provide: MAT_DIALOG_DEFAULT_OPTIONS, useValue: {width: '450px'}},
  ],
  bootstrap: [AppComponent]
})
export class AppModule {
}
