import { Injectable } from '@angular/core';
import { AbstractControl, ValidatorFn } from '@angular/forms';

@Injectable({
  providedIn: 'root'
})
export class ValidatorService {

  constructor() { }

  checkName(cant: string[]): ValidatorFn {
    return (control: AbstractControl): { [key: string]: boolean } | null => {
      if (cant.filter(a => a === control.value).length !== 0) {
        return { nameExists: true };
      }
      return null;
    };
  }

  special(): ValidatorFn {
    return (control: AbstractControl): { [key: string]: boolean } | null => {
      const nameRegexp = /[\\/:*?"<>|]+/g;
      if (nameRegexp.test(control.value)) {
        return { special: true };
      }
      return null;
    };
  }
}
