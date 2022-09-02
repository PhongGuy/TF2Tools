import {Component, ElementRef, OnInit, ViewChild} from '@angular/core';
import {FormControl} from '@angular/forms';
import {MatDialog} from '@angular/material/dialog';
import {map, Observable, startWith} from 'rxjs';
import {AppComponent} from '../../app.component';
import {ElectronService} from '../../core/services';
import {QuestionAnswerComponent} from '../../dialogs/question-answer/question-answer.component';
import {YesNoComponent} from '../../dialogs/yes-no/yes-no.component';
import {Hud} from '../../models/hud';
import {QuestionAnswer} from '../../models/questionAnswer';
import {YesNo} from '../../models/yesNo';
import {FileHelpService} from '../../services/file-help.service';
import {LogService} from '../../services/log.service';
import {SnackService} from '../../services/snack.service';

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
   * Installed huds of hud component
   */
  installedHuds: Hud[];
  /**
   * Library  of hud component
   */
  library: Hud[];
  /**
   * Local huds of hud component
   */
  localHuds: string;
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
   * @param log
   * @param fileHelp
   */
  constructor(
    private electron: ElectronService,
    public app: AppComponent,
    private dialog: MatDialog,
    private snack: SnackService,
    private log: LogService,
    private fileHelp: FileHelpService
  ) {
    this.localHuds = `${this.app.settings.libraryPath}\\huds`;
    this.electron.fs.ensureDirSync(this.localHuds);
    this.log.scope('HUDS');
  }

  /**
   * on init
   *
   * @description first we need to look for a folder with info file in, we will assume that is the current hud.
   */
  ngOnInit(): void {
    this.getHudsAndAddThemToLibraryAndInstalledHuds();

    // temp, to move old files
    if (this.library.length === 0) {
      let oldPath = this.electron.appData('TF2Tools\\huds');
      if (this.electron.fs.existsSync(oldPath)) {
        this.snack.show('Moving your files to new library...', null, 5000);
        this.log.info('MOVE', `Trying to move files from "${oldPath}" => "${this.localHuds}"`);
        this.electron.fs.move(oldPath, this.localHuds, {overwrite: true})
          .then(() => {
            oldPath = this.electron.appData('TF2Tools\\hitsounds');
            if (this.electron.fs.existsSync(oldPath)) {
              this.log.info('MOVE', `Trying to move your hitsounds from "${oldPath}" => "${this.app.settings.libraryPath}\\hitsounds"`);
              this.electron.fs.move(oldPath, `${this.app.settings.libraryPath}\\hitsounds`, {overwrite: true})
                .then(() => {
                  this.snack.show('Files was moved to new library');
                  this.getHudsAndAddThemToLibraryAndInstalledHuds();
                }).catch((err) => this.log.error('MOVE', err));
            }
          }).catch((err) => this.log.error('MOVE', err));
      }
    }


    this.filteredOptions = this.myControl.valueChanges.pipe(
      startWith(''),
      map(value => this.hudFilter(value)),
    );
  }

  /**
   * Gets huds and add them to library and installed huds
   */
  getHudsAndAddThemToLibraryAndInstalledHuds(): void {
    this.library = [];
    this.electron.fs.ensureDirSync(this.localHuds);
    this.log.info('READ', `Looking for huds in ${this.localHuds}`);
    this.electron.fs.readdirSync(this.localHuds).forEach(async customFiles => {
      const info = this.localHuds + '/' + customFiles + '/info.vdf';
      if (this.electron.fs.existsSync(info)) {
        this.addHudToLibraryIfMissing(customFiles);
      }
    });

    this.installedHuds = [];
    this.electron.fs.readdirSync(this.app.settings.customPath).forEach(customFiles => {
      const info = this.app.settings.customPath + '/' + customFiles + '/info.vdf';
      if (this.electron.fs.existsSync(info)) {
        this.addHudToInstalledHuds(customFiles);
      }
    });

    this.myControl.setValue('');
  }

  /**
   * Installs hud
   *
   * @param _hud
   */
  async installHud(_hud: Hud): Promise<void> {
    this.app.loading = true;
    this.log.info('COPY', `Installing "${_hud.path}" => "${this.app.settings.customPath}\\${_hud.name}"`);
    this.makeSureThereAreNoReadOnlyFoldersInThisFolder(_hud.path);
    const dest = `${this.app.settings.customPath}\\${_hud.name}`;
    this.electron.fs.copy(_hud.path, dest)
      .then(() => {
        this.snack.show(`${_hud.name} was installed`);
        this.addHudToInstalledHuds(_hud.name);
        this.app.loading = false;
      }).catch(err => this.log.error('COPY', err));
  }

  /**
   * Uninstalls hud
   *
   * @param _hud
   * @param [loadingStop] this is meant to be used with `replaceHudWithInstalled`
   */
  uninstallHud(_hud: Hud, loadingStop = true): void {
    if (this.electron.fs.existsSync(_hud.path)) {
      this.app.loading = true;
      if (this.app.settings.moveOrCopy) {
        this.log.info('MOVE', `Uninstalling "${_hud.path}" => "${this.localHuds}\\${_hud.name}"`);
        const dest = `${this.localHuds}\\${_hud.name}`;
        this.electron.fs.move(_hud.path, dest, {overwrite: true})
          .then(_ => {
            this.snack.show(`${_hud.name} was uninstalled`);
            this.removeHudFromInstalledHuds(_hud);
            this.addHudToLibraryIfMissing(_hud.name);
            if (loadingStop) {
              this.app.loading = false;
            }
          }).catch(err => this.log.error('MOVE', err));
      } else {
        this.log.info('REMOVE', `Uninstalling "${_hud.path}"`);
        this.electron.fs.remove(_hud.path)
          .then(_ => {
            this.snack.show(`${_hud.name} was uninstalled`);
            this.removeHudFromInstalledHuds(_hud);
            if (loadingStop) {
              this.app.loading = false;
            }
          }).catch(err => this.log.error('REMOVE', err));
      }
    }
  }

  /**
   * Deletes hud
   *
   * @param _hud
   */
  deleteHud(_hud: Hud): void {
    if (this.electron.fs.existsSync(_hud.path)) {
      this.app.loading = true;
      const d: YesNo = new YesNo();
      d.question = `Delete ${_hud.name}?`;
      d.subQuestion = `Are you sure you want to delete ${_hud.name}? This cannot be undone!`;

      const dialogRef = this.dialog.open(YesNoComponent, {
        data: d
      });

      dialogRef.afterClosed().subscribe(result => {
        if (result) {
          this.log.info('DELETE', `Deleted "${_hud.path}"`);
          this.electron.fs.remove(_hud.path)
            .then(() => {
              this.snack.show(`${_hud.name} was deleted`);
              this.removeHudFromLibrary(_hud);
              this.app.loading = false;
            }).catch(err => this.log.error('REMOVE', err));
        } else {
          this.app.loading = false;
        }
      });
    }
  }

  test() {
    this.app.loading = false;
  }

  /**
   * Uploads huds
   *
   * @param event
   */
  uploadHuds(event: Event): void {
    this.app.loading = true;
    const target = event.target as HTMLInputElement;
    const files: File[] = Array.from(target.files);
    const hudsFound = this.findHudsInFiles(files);
    if (hudsFound.length > 0) {
      hudsFound.forEach(hud => {
        const dest = `${this.localHuds}\\${hud.name}`;

        if (!this.electron.fs.existsSync(dest)) {
          this.log.info('COPY', `Uploading "${hud.path}" => "${dest}"`);
          try {
            this.electron.fs.copySync(hud.path, dest);
            this.log.info('COPY', `Successfully uploaded ${hud.name}`);
            this.snack.show(`Added "${hud.name}" to library`, null, hudsFound.length > 2 ? 1000 : 2600);
            this.addHudToLibraryIfMissing(hud.name);
          } catch (err) {
            this.log.error('COPY', err);
          }
        } else {
          this.snack.show(`${hud.name} is already in library`);
        }
      });
      this.app.loading = false;
    } else {
      const s = 'We could not find any hud here. Are you sure you selected the right folder and it has the info.vdf file inside.';
      this.snack.show(s, null, 6000);
      this.app.loading = false;
    }
    this.folderUpload.nativeElement.value = null;
  }

  /**
   * Renames hud
   *
   * @param _hud
   */
  renameHud(_hud: Hud): void {
    const dialogRef = this.dialog.open(QuestionAnswerComponent, {
      data: {
        question: `Rename ${_hud.name}`,
        subQuestion: '',
        cant: this.getNames(_hud.name)
      } as QuestionAnswer
    });

    dialogRef.afterClosed().subscribe(newName => {
      if (typeof newName === 'string') {
        this.log.info('RENAME', `Renaming "${_hud.name}" => "${newName}"`);
        const dest = `${this.localHuds}\\${newName}`;

        this.electron.fs.rename(_hud.path, dest)
          .then(() => {
            this.snack.show(`Updated "${_hud.name}" to "${newName}"`);
            this.renameLibraryHud(_hud, newName);
          }).catch(err => this.log.error('RENAME', err));
      }
    });
  }

  /**
   * Replaces hud with installed
   *
   * @param _hud
   */
  async replaceHudWithInstalled(_hud: Hud) {
    this.installedHuds.forEach(hud => {
      this.uninstallHud(hud, false);
    });
    await this.installHud(_hud);
  }

  /**
   * Finds huds in files
   *
   * @param files
   * @returns
   */
  private findHudsInFiles(files: File[]): Hud[] {
    const hudsFound: Hud[] = [];

    const huds = files.filter(file => file.name.toLowerCase() === 'info.vdf');

    huds.forEach(hud => {
      const path = hud.path.replace('\\info.vdf', '');
      const name = path.split('\\').pop();
      hudsFound.push({name, path});
    });

    return hudsFound;
  }

  /**
   * Adds hud to installed huds
   *
   * @param name
   */
  private addHudToInstalledHuds(name: string): void {
    const hud = new Hud();
    hud.name = name;
    hud.path = `${this.app.settings.customPath}\\${name}`;
    this.installedHuds.push(hud);
  }

  /**
   * Removes hud from installed huds
   *
   * @param _hud
   */
  private removeHudFromInstalledHuds(_hud: Hud): void {
    const index = this.installedHuds.indexOf(_hud);
    if (index >= 0) {
      this.installedHuds.splice(index, 1);
    }
  }

  /**
   * Adds hud to library if missing
   *
   * @param name
   * @param name
   */
  private addHudToLibraryIfMissing(name: string) {

    const hud = new Hud();

    hud.name = name;
    hud.path = `${this.localHuds}\\${name}`;

    const index = this.library.findIndex(a => a.name === name);
    if (index === -1) {
      this.library.push(hud);
      this.myControl.setValue('');
    }
  }

  /**
   * Renames library hud
   *
   * @param _hud
   * @param newName
   */
  private renameLibraryHud(_hud: Hud, newName: string): void {
    const index = this.library.indexOf(_hud);
    if (index >= 0) {
      this.library[index].name = newName;
      this.library[index].path = `${this.localHuds}\\${newName}`;
    }
  }

  /**
   * Removes hud from library
   *
   * @param _hud
   */
  private removeHudFromLibrary(_hud: Hud): void {
    const index = this.library.indexOf(_hud);
    if (index >= 0) {
      this.library.splice(index, 1);
      this.myControl.setValue('');
    }
  }

  /**
   * Makes sure there are no read only folders in this folder
   *
   * https://www.geeksforgeeks.org/node-js-fs-chmod-method/?ref=lbp
   *
   * @param path
   */
  private makeSureThereAreNoReadOnlyFoldersInThisFolder(path: string): void {
    const foldersAlreadyLookedAt = [];
    const files = this.fileHelp.getAllFiles(path);
    files.forEach(file => {
      const folder = file.split('\\');
      folder.pop();
      const folderPath = folder.join('\\');
      if (!foldersAlreadyLookedAt.includes(folderPath)) {
        const mode = this.electron.fs.statSync(folderPath).mode;
        if (mode !== 16822) {
          this.log.warn('CHANGE', `Have to change "${folderPath}" from "${mode}" => "16822"`);
          this.electron.fs.chmodSync(folderPath, 0o600);
        }
        foldersAlreadyLookedAt.push(folderPath);
      }
    });
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
      if (option.name.toLowerCase().includes(filterValue)) {
        return true;
      }
    }).sort((a: Hud, b: Hud) => a.name.localeCompare(b.name));
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
      if (a.name !== exclude) {
        names.push(a.name);
      }
    });
    return names;
  }
}
