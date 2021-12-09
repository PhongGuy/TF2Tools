import { HttpClient, HttpClientModule } from '@angular/common/http';
import { NgModule } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { BrowserModule } from '@angular/platform-browser';
import { BrowserAnimationsModule } from '@angular/platform-browser/animations';
// NG Translate
import { TranslateLoader, TranslateModule } from '@ngx-translate/core';
import { TranslateHttpLoader } from '@ngx-translate/http-loader';
import { AppRoutingModule } from './app-routing.module';
import { AppComponent } from './app.component';
import { CoreModule } from './core/core.module';
import { DashboardComponent } from './dashboard/dashboard.component';
import { HitsoundComponent } from './dashboard/hitsound/hitsound.component';
import { HudComponent } from './dashboard/hud/hud.component';
import { MaterialModule } from './material';
import { SharedModule } from './shared/shared.module';
import { QuestionAnswerComponent } from './dialogs/question-answer/question-answer.component';
import { YesNoComponent } from './dialogs/yes-no/yes-no.component';
import { CrosshairsComponent } from './dashboard/crosshairs/crosshairs.component';
import { WeaponsSoundsComponent } from './dashboard/weapons-sounds/weapons-sounds.component';

// AoT requires an exported function for factories
const httpLoaderFactory = (http: HttpClient): TranslateHttpLoader => new TranslateHttpLoader(http, './assets/i18n/', '.json');

@NgModule({
  declarations: [
    AppComponent,
    DashboardComponent,
    HudComponent,
    HitsoundComponent,
    QuestionAnswerComponent,
    YesNoComponent,
    CrosshairsComponent,
    WeaponsSoundsComponent
  ],
  imports: [
    BrowserModule,
    BrowserAnimationsModule,
    FormsModule,
    HttpClientModule,
    CoreModule,
    SharedModule,
    MaterialModule,
    AppRoutingModule,
    TranslateModule.forRoot({
      loader: {
        provide: TranslateLoader,
        useFactory: httpLoaderFactory,
        deps: [HttpClient]
      }
    })
  ],
  providers: [],
  bootstrap: [AppComponent]
})
export class AppModule { }
