import { Injectable } from '@angular/core';
import { ElectronService } from '../core/services';

@Injectable({
  providedIn: 'root'
})
export class FileHelpService {

  constructor(
    private electron: ElectronService
  ) { }

  getAllFiles(dirPath, arrayOfFiles = []): string[] {
    const files = this.electron.fs.readdirSync(dirPath);

    files.forEach(file => {
      if (this.electron.fs.statSync(dirPath + '/' + file).isDirectory()) {
        arrayOfFiles = this.getAllFiles(dirPath + '/' + file, arrayOfFiles);
      } else {
        arrayOfFiles.push(this.electron.path.join(dirPath, '/', file));
      }
    });

    return arrayOfFiles;
  }
}
