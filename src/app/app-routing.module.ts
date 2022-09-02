import { NgModule } from '@angular/core';
import { RouterModule, Routes } from '@angular/router';
import { CrosshairsComponent } from './dashboard/crosshairs/crosshairs.component';
import { DashboardComponent } from './dashboard/dashboard.component';
import { HitsoundComponent } from './dashboard/hitsound/hitsound.component';
import { HudComponent } from './dashboard/hud/hud.component';
import { SettingsComponent } from './dashboard/settings/settings.component';
import { SetupComponent } from './setup/setup.component';
import {WeaponSoundsComponent} from './dashboard/game-sounds/weapon-sounds/weapon-sounds.component';

const routes: Routes = [
  {
    path: 'dashboard',
    component: DashboardComponent,
    children: [
      {
        path: 'hud',
        component: HudComponent
      },
      {
        path: 'hitsound',
        component: HitsoundComponent
      },
      {
        path: 'crosshairs',
        component: CrosshairsComponent
      },
      {
        path: 'weapon-sounds',
        component: WeaponSoundsComponent
      },
      {
        path: 'settings',
        component: SettingsComponent
      }
    ]
  },
  {
    path: 'setup',
    component: SetupComponent
  }
];

/**
 * App Routing Module
 */
@NgModule({
  imports: [
    RouterModule.forRoot(routes, { relativeLinkResolution: 'legacy' }),
  ],
  exports: [RouterModule]
})
export class AppRoutingModule { }
