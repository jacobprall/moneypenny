class CreateBills < ActiveRecord::Migration[5.2]
  def change
    create_table :bills do |t|
      t.string :name, null: false
      t.float :amount, null: false
      t.date :due_date, null: false
      t.boolean :recurring, null: false
      t.integer :user_id, null: false
      t.timestamps
    end
    add_index :bills, :user_id
  end
end
