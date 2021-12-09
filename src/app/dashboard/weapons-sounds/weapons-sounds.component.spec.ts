import { ComponentFixture, TestBed } from '@angular/core/testing';

import { WeaponsSoundsComponent } from './weapons-sounds.component';

describe('WeaponsSoundsComponent', () => {
  let component: WeaponsSoundsComponent;
  let fixture: ComponentFixture<WeaponsSoundsComponent>;

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      declarations: [ WeaponsSoundsComponent ]
    })
    .compileComponents();
  });

  beforeEach(() => {
    fixture = TestBed.createComponent(WeaponsSoundsComponent);
    component = fixture.componentInstance;
    fixture.detectChanges();
  });

  it('should create', () => {
    expect(component).toBeTruthy();
  });
});
