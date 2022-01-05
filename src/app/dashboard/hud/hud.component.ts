import { Component, ElementRef, OnInit, ViewChild } from '@angular/core';
import { FormControl } from '@angular/forms';
import { MatDialog } from '@angular/material/dialog';
import { map, Observable, startWith } from 'rxjs';
import { AppComponent } from '../../app.component';
import { ElectronService } from '../../core/services';
import { QuestionAnswerComponent } from '../../dialogs/question-answer/question-answer.component';
import { YesNoComponent } from '../../dialogs/yes-no/yes-no.component';
import { Hud } from '../../models/hud';
import { QuestionAnswer } from '../../models/questionAnswer';
import { YesNo } from '../../models/yesNo';
import { SnackService } from '../../services/snack.service';

/**
 * Hud
 */
@Component({
  selector: 'app-hud',
  templateUrl: './hud.component.html'
})
export class HudComponent implements OnInit {

  /**
   * Folder upload `ElementRef`
   */
  @ViewChild('folderUpload') folderUpload: ElementRef;

  /**
   * Current huds of hud component
   */
  currentHuds: Hud[];
  /**
   * Huds installed of hud component
   */
  hudsInstalled: number;
  /**
   * Library  of hud component
   */
  library: Hud[];
  /**
   * Local huds of hud component
   */
  localHuds: string;
  /**
   * Not in library of hud component
   */
  notInLibrary: Hud[];

  /**
   * My control of hud component
   */
  myControl = new FormControl();
  /**
   * Filtered options of hud component
   */
  filteredOptions: Observable<Hud[]>;

  /**
   * Creates an instance of hud component.
   *
   * @param electron
   * @param app
   * @param dialog
   * @param snack
   */
  constructor(
    private electron: ElectronService,
    private app: AppComponent,
    private dialog: MatDialog,
    private snack: SnackService
  ) {
    this.localHuds = `${this.app.settings.libraryPath}\\huds`;
    this.electron.fs.ensureDir(this.localHuds);
  }

  /**
   * on init
   *
   * @description first we need to look for a folder with info file in, we will assume that is the current hud.
   */
  ngOnInit(): void {
    this.update();

    // temp, to move old files
    if (this.library.length === 0) {
      let oldPath = this.electron.appData('TF2Tools\\huds');
      if (this.electron.fs.existsSync(oldPath)) {
        this.snack.show('Moving your files to new library...', null, 5000);
        this.app.log.next(`Trying to move files from "${oldPath}" => "${this.localHuds}"`);
        this.electron.fs.move(oldPath, this.localHuds, { overwrite: true })
          .then(() => {
            oldPath = this.electron.appData('TF2Tools\\hitsounds');
            if (this.electron.fs.existsSync(oldPath)) {
              this.app.log.next(`Trying to move your hitsounds from "${oldPath}" => "${this.app.settings.libraryPath}\\hitsounds"`);
              this.electron.fs.move(oldPath, `${this.app.settings.libraryPath}\\hitsounds`, { overwrite: true })
                .then(() => {
                  this.snack.show('Files was moved to new library');
                  this.update();
                });
            }
          });
      }
    }


    this.filteredOptions = this.myControl.valueChanges.pipe(
      startWith(''),
      map(value => this.hudFilter(value)),
    );
  }

  /**
   * Updates hud component
   */
  update(): void {
    this.library = [];
    this.electron.fs.ensureDir(this.localHuds);

    this.electron.fs.readdirSync(this.localHuds).forEach(customFiles => {
      const info = this.localHuds + '/' + customFiles + '/info.vdf';
      if (this.electron.fs.existsSync(info)) {
        const data = this.electron.fs.readFileSync(info, { encoding: 'utf8', flag: 'r' }).split('"');
        const h = { name: data[1], folderName: customFiles, version: data[5], path: this.localHuds + '\\' + customFiles };
        this.library.push(h);
      }
    });

    this.hudsInstalled = 0;
    this.currentHuds = [];
    this.notInLibrary = [];
    this.electron.fs.readdirSync(this.app.settings.customPath).forEach(customFiles => {
      const info = this.app.settings.customPath + '/' + customFiles + '/info.vdf';
      if (this.electron.fs.existsSync(info)) {
        this.hudsInstalled++;

        const data = this.electron.fs.readFileSync(info, { encoding: 'utf8', flag: 'r' }).split('"');
        const h = { name: data[1], folderName: customFiles, version: data[5], path: this.app.settings.customPath + '\\' + customFiles };
        const l = { name: data[1], folderName: customFiles, version: data[5], path: this.localHuds + '\\' + customFiles };

        this.currentHuds.push(h);

        if (!this.electron.fs.existsSync(l.path)) {
          this.notInLibrary.push(h);
        }
      }
    });

    this.myControl.setValue('');
  }

  /**
   * Adds hud
   *
   * @param _hud
   */
  add(_hud: Hud): void {
    this.app.log.next(`Installing hud: *COPY* "${_hud.path}" => "${this.app.settings.customPath}\\${_hud.folderName}"`);
    this.electron.fs.copy(_hud.path, `${this.app.settings.customPath}\\${_hud.folderName}`)
      .then(() => {
        this.snack.show(`${_hud.folderName} was installed`);
        this.update();
      })
      .catch(err => this.app.error(err));
  }

  /**
   * Uninstalls hud
   *
   * @param _hud
   */
  uninstall(_hud: Hud): void {
    if (this.electron.fs.existsSync(_hud.path)) {
      if (this.app.settings.moveOrCopy) {
        this.app.log.next(`Uninstalling hud: *MOVE* "${_hud.path}" => "${this.localHuds}\\${_hud.folderName}"`);
        this.electron.fs.move(_hud.path, `${this.localHuds}\\${_hud.folderName}`, { overwrite: true })
          .then(() => {
            this.snack.show(`${_hud.folderName} was uninstalled`);
            this.update();
          })
          .catch(err => this.app.error(err));
      } else {
        this.app.log.next(`Uninstalling hud: *DELETE* "${_hud.path}"`);
        this.electron.fs.remove(_hud.path)
          .then(() => {
            this.snack.show(`${_hud.folderName} was uninstalled`);
            this.update();
          });
      }
    }
  }

  /**
   * Removes hud
   *
   * @param _hud
   */
  remove(_hud: Hud): void {
    if (this.electron.fs.existsSync(_hud.path)) {
      const d: YesNo = new YesNo();
      d.question = `Remove ${_hud.folderName}?`;
      d.subQuestion = `Are you sure you want to remove ${_hud.folderName}? This cannot be undone!`;

      const dialogRef = this.dialog.open(YesNoComponent, {
        width: '450px',
        data: d
      });

      dialogRef.afterClosed().subscribe(result => {
        if (result) {
          this.app.log.next(`Remove hud: *DELETE* "${_hud.path}"`);
          this.electron.fs.remove(_hud.path)
            .then(() => {
              this.snack.show(`${_hud.folderName} was removed`);
              this.update();
            })
            .catch(err => this.app.error(err));
        }
      });
    }
  }

  /**
   * Uploads huds
   *
   * @param event
   */
  upload(event: Event): void {
    const target = event.target as HTMLInputElement;
    const files: File[] = Array.from(target.files);
    const hudsFound = files.filter(i => i.name === 'info.vdf');
    if (hudsFound.length > 0) {
      hudsFound.forEach(hud => {
        const path = hud.path.replace('\\info.vdf', '');
        const name = path.split('\\').pop();
        const dest = `${this.localHuds}\\${name}`;

        if (!this.electron.fs.existsSync(dest)) {
          this.app.log.next(`Uploading hud: *COPY* "${path}" => "${dest}"`);
          this.electron.fs.copy(path, dest)
            .then(() => {
              this.app.log.next(`Successfully uploaded ${name}`);
              this.snack.show(`Added "${name}" to library`, null, hudsFound.length > 2 ? 1000 : 3000);
              const data = this.electron.fs.readFileSync(hud.path, { encoding: 'utf8', flag: 'r' }).split('"');
              const l = { name: data[1], folderName: name, version: data[5], path: this.localHuds + '\\' + name };
              this.library.push(l);
              this.myControl.setValue('');
            }).catch(err => this.app.error(err));
        } else {
          this.snack.show(`${name} is already in library`);
        }
      });
    } else {
      const s = 'We could not find any hud here. Are you sure you selected the right folder and it has the info.vdf file inside.';
      this.snack.show(s, null, 6000);
    }

    this.folderUpload.nativeElement.value = null;
  }

  /**
   * Renames hud
   *
   * @param _hud
   */
  rename(_hud: Hud): void {
    const dialogRef = this.dialog.open(QuestionAnswerComponent, {
      width: '450px',
      data: {
        question: `Rename ${_hud.folderName}`,
        subQuestion: '',
        cant: this.getNames(_hud.folderName)
      } as QuestionAnswer
    });

    dialogRef.afterClosed().subscribe(result => {
      if (typeof result === 'string') {
        const path = _hud.path.split('\\');
        const name = path.pop();
        this.app.log.next(`Renaming hud: *RENAME* "${_hud.folderName}" => "${name}"`);
        this.electron.fs.rename(_hud.path, `${path.join('\\')}\\${result}`)
          .then(() => {
            this.snack.show(`Updated "${name}" to "${result}"`);
            this.update();
          })
          .catch(err => this.app.error(err));
      }
    });
  }

  /**
   * Replaces hud component
   *
   * @param _hud
   */
  replace(_hud: Hud): void {
    this.currentHuds.forEach(hud => {
      this.uninstall(hud);
    });
    this.add(_hud);
  }

  /**
   * Huds filter
   *
   * @param value
   * @returns filter
   */
  private hudFilter(value: string): Hud[] {
    const filterValue = value.toLowerCase();

    return this.library.filter(option => {
      if (option.folderName.toLowerCase().includes(filterValue)) {
        return true;
      } else if (option.name.toLowerCase().includes(filterValue)) {
        return true;
      }
    });
  }

  /**
   * Gets names
   *
   * @param exclude
   * @returns names
   */
  private getNames(exclude: string): string[] {
    const names: string[] = [];
    this.library.forEach(a => {
      if (a.folderName !== exclude) {
        names.push(a.folderName);
      }
    });
    return names;
  }
}
