class ChangeTable < ActiveRecord::Migration[5.2]
  def change
    remove_column :users, :fname, :lname
    add_column :users, :p_num, :integer, null: false
  end
end
