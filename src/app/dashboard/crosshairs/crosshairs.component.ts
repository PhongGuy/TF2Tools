import { Component, OnInit } from '@angular/core';
import { APP_CONFIG } from '../../../environments/environment';
import { AppComponent } from '../../app.component';
import { ElectronService } from '../../core/services';
import { SnackService } from '../../services/snack.service';

@Component({
  selector: 'app-crosshairs',
  templateUrl: './crosshairs.component.html',
  styleUrls: ['./crosshairs.component.scss']
})
export class CrosshairsComponent implements OnInit {

  constructor(
    public app: AppComponent,
    private electron: ElectronService,
    private snack: SnackService,
  ) {
  }

  ngOnInit(): void {
  }

  generate(): void {
    const vtfPath = `${this.app.path.custom}\\mycustomstuff\\materials\\vgui\\replay\\thumbnails`;
    const scriptPath = `${this.app.path.custom}\\mycustomstuff\\scripts`;
    this.electron.fs.ensureDir(vtfPath)
      .then(() => {
        this.snack.show('Adding vtf crosshairs', null, 2500);
        this.electron.fs.copy(`${APP_CONFIG.src}assets/vtf/materials/vgui/replay/thumbnails`, vtfPath, { overwrite: true })
          .then(() => {
            this.electron.fs.ensureDir(scriptPath)
              .then(() => {
                this.electron.fs.copy(`${APP_CONFIG.src}assets/vtf/scripts`, scriptPath, { overwrite: false })
                  .then(() => {
                    this.snack.show('vtf crosshairs was generated');
                    this.app.update();
                  })
                  .catch((err) => this.snack.show(err, null, 9000));
              });
          })
          .catch((err) => this.snack.show(err, null, 9000));
      })
      .catch((err) => this.snack.show(err, null, 9000));
  }

}
