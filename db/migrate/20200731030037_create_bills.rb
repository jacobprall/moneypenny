class CreateBills < ActiveRecord::Migration[5.2]
  def change
    create_table :bills do |t|
      t.string :name, null: false
      t.string :details
      t.decimal :amount_due, null: false, precision: 8, scale: 2
      t.integer :recurring, null: false
      t.datetime :due_date
      t.integer :user_id
      t.timestamps
    end
    add_index :bills, [:user_id]
  end
end
