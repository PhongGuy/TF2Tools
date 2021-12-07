import { NgModule } from '@angular/core';
import { RouterModule, Routes } from '@angular/router';
import { CrosshairsComponent } from './dashboard/crosshairs/crosshairs.component';
import { DashboardComponent } from './dashboard/dashboard.component';
import { HitsoundComponent } from './dashboard/hitsound/hitsound.component';
import { HudComponent } from './dashboard/hud/hud.component';

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
      }
    ]
  }
];

@NgModule({
  imports: [
    RouterModule.forRoot(routes, { relativeLinkResolution: 'legacy' }),
  ],
  exports: [RouterModule]
})
export class AppRoutingModule { }
